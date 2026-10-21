use crate::{
    BridgeConfig, BridgeState,
    config::phone_hex,
    herdr::{self, Agent},
    output,
    state::Mapping,
    wire::{PROTOCOL, Socket, read_line},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Bridge {
    pub config: BridgeConfig,
    pub state: BridgeState,
    pub agents: BTreeMap<String, Agent>,
    pub herdr: Socket,
    pub marmot: Socket,
    pub herdr_available: bool,
    phone: String,
}

impl Bridge {
    pub fn open(config: BridgeConfig) -> Result<Self> {
        let mut state = BridgeState::load(&config.state_path)?;
        let phone = phone_hex(&config.phone_npub)?;
        ensure!(
            state
                .account
                .as_ref()
                .is_none_or(|a| a == &config.account_id_hex)
                && state.phone.as_ref().is_none_or(|p| p == &phone),
            "State belongs to another account or phone"
        );
        state.account = Some(config.account_id_hex.clone());
        state.phone = Some(phone.clone());
        state.save(&config.state_path)?;
        Ok(Self {
            herdr: Socket {
                path: config.herdr_socket.clone(),
            },
            marmot: Socket {
                path: config.wn_agent_socket.clone(),
            },
            config,
            state,
            agents: BTreeMap::new(),
            herdr_available: false,
            phone,
        })
    }

    pub fn save(&self) -> Result<()> {
        self.state.save(&self.config.state_path)
    }

    pub async fn flush(&mut self) -> Result<()> {
        let mut failed = HashSet::new();
        for item in self.state.outbox.clone() {
            if failed.contains(&item.group) {
                continue;
            }
            match self.marmot.marmot(json!({"type":"send_final","account_id_hex":self.config.account_id_hex,"group_id_hex":item.group,"text":item.text,"reply_to_message_id_hex":null,"idempotency_key":item.id})).await {
                Ok(response) if response["type"] == "final_sent" && response["message_ids_hex"].as_array().is_some_and(|ids|!ids.is_empty()) => {
                    self.state.outbox.retain(|m| m.id != item.id); self.save()?;
                }
                _ => { failed.insert(item.group); }
            }
        }
        ensure!(failed.is_empty(), "Some group deliveries remain pending");
        Ok(())
    }

    pub async fn reconcile(&mut self, agents: Vec<Agent>) -> Result<()> {
        let mut current = BTreeMap::new();
        let mut uncertain_terminals = HashSet::new();
        for mut agent in agents {
            // Never attach a group to a location that a replacement session can reuse.
            let Some(session) = agent.agent_session.as_ref() else {
                uncertain_terminals.insert(agent.terminal_id.clone());
                continue;
            };
            if agent.agent.is_empty() {
                agent.agent = session.agent.clone();
                agent.agent_status = "unknown".into();
            }
            ensure!(
                current.insert(agent.key(), agent).is_none(),
                "Two live agents report the same session identity"
            );
        }
        self.agents = current;
        self.herdr_available = true;
        let keys: Vec<_> = self.agents.keys().cloned().collect();
        for key in keys {
            let agent = self.agents[&key].clone();
            if self.reconcile_agent(&key, &agent).await.is_err() {
                eprintln!("A session could not reconcile; other sessions remain available.");
            }
        }
        let ended: Vec<_> = self
            .state
            .mappings
            .iter()
            .filter(|(key, m)| {
                m.active
                    && !self.agents.contains_key(*key)
                    && !m
                        .agent_record
                        .as_ref()
                        .is_some_and(|a| uncertain_terminals.contains(&a.terminal_id))
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in ended {
            if let Some(group) = self.state.mappings[&key].group_id_hex.clone() {
                if let Some(agent) = self.state.mappings[&key].agent_record.clone()
                    && self.collect_finals(&key, &agent, &group).await.is_err()
                {
                    eprintln!("Final reply for an ended session remains pending.");
                    continue;
                }
                self.state.enqueue(&group, "Session ended. This group retains its history. A restored native session will reconnect here.");
            }
            let m = self.state.mappings.get_mut(&key).unwrap();
            m.active = false;
            m.pending_prompt = false;
        }
        self.save()
    }

    async fn seed_native_history(&mut self, key: &str, agent: &Agent) -> Result<bool> {
        if !self.state.mappings[key].output_initialised
            && let Some(session) = agent
                .agent_session
                .as_ref()
                .filter(|session| matches!(session.agent.as_str(), "codex" | "claude"))
        {
            let finals = output::read_session(&self.config, session).await?;
            let mapping = self.state.mappings.get_mut(key).unwrap();
            mapping
                .final_ids
                .extend(finals.into_iter().map(|reply| reply.id));
            mapping.output_initialised = true;
            mapping.last_export_at = now();
            self.save()?;
            return Ok(true);
        }
        Ok(false)
    }

    async fn collect_finals(&mut self, key: &str, agent: &Agent, group: &str) -> Result<()> {
        // The ended-session path also reaches this method without live reconciliation.
        if self.seed_native_history(key, agent).await? {
            return Ok(());
        }
        if let Some(session) = agent
            .agent_session
            .as_ref()
            .filter(|s| output::supported(&s.agent))
        {
            let finals = output::read_session(&self.config, session).await?;
            let known: HashSet<_> = self.state.mappings[key].final_ids.iter().cloned().collect();
            for reply in finals {
                if !known.contains(&reply.id) {
                    self.state.enqueue(group, &reply.text);
                    self.state
                        .mappings
                        .get_mut(key)
                        .unwrap()
                        .final_ids
                        .push(reply.id);
                }
            }
            self.state.mappings.get_mut(key).unwrap().last_export_at = now();
        }
        Ok(())
    }

    async fn reconcile_agent(&mut self, key: &str, agent: &Agent) -> Result<()> {
        if !self.state.mappings.contains_key(key) {
            // Capture the initial final-message watermark before exposing a new group.
            let mut m = Mapping {
                joined_at: now(),
                active: true,
                agent_record: Some(agent.clone()),
                ..Mapping::default()
            };
            if let Some(session) = agent
                .agent_session
                .as_ref()
                .filter(|s| output::supported(&s.agent))
            {
                let finals = output::read_session(&self.config, session).await?;
                m.final_ids = finals.into_iter().map(|f| f.id).collect();
                m.output_initialised = true;
            }
            self.state.mappings.insert(key.to_owned(), m);
            self.save()?;
        }
        // Older bridge versions created these groups without a transcript watermark.
        // Seed their existing history once when enabling native reply support.
        self.seed_native_history(key, agent).await?;
        let m = &self.state.mappings[key];
        if m.group_id_hex.is_none() && !m.creating {
            self.state.mappings.get_mut(key).unwrap().creating = true;
            self.save()?;
            // No blind retries: GroupCreate has no idempotency key, even after a timeout.
            match self.marmot.marmot(json!({"type":"group_create", "account_id_hex":self.config.account_id_hex,"name":agent.group_name(),"members":[self.config.phone_npub],"description":"Remote for one HERDR agent session."})).await {
                    Ok(response) => {
                        ensure!(response["type"] == "group_created", "Unexpected group creation response");
                        let group = response["group_id_hex"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing group ID"))?.to_owned();
                        let m = self.state.mappings.get_mut(key).unwrap(); m.group_id_hex = Some(group.clone()); m.creating = false;
                        self.state.enqueue(&group, &format!("Connected to HERDR: {}. Send a message here to prompt this session. Use /status for its state.", agent.title()));
                        self.save()?;
                    }
                    Err(error) => {
                        if error.downcast_ref::<crate::wire::NotSubmitted>().is_some() {
                            self.state.mappings.get_mut(key).unwrap().creating = false; self.save()?;
                        } else { eprintln!("Group creation needs inspection. Use status and resolve-create before retrying."); }
                        return Ok(());
                    }
                }
        }
        let Some(group) = self.state.mappings[key].group_id_hex.clone() else {
            return Ok(());
        };
        let previous = &self.state.mappings[key];
        let changed = previous.last_agent_status != agent.agent_status
            || previous.last_state_change_seq != agent.state_change_seq;
        if previous.last_agent_status != agent.agent_status && agent.agent_status != "done" {
            self.state
                .enqueue(&group, &format!("State: {}", agent.agent_status));
        }
        let m = self.state.mappings.get_mut(key).unwrap();
        m.last_agent_status = agent.agent_status.clone();
        m.last_state_change_seq = agent.state_change_seq;
        m.active = true;
        m.agent_record = Some(agent.clone());
        if changed && agent.settled() {
            m.completion_pending = true;
        }
        self.save()?;
        if agent.agent_status == "blocked"
            && self.state.mappings[key].blocked_context_seq != Some(agent.state_change_seq)
        {
            let response = self
                .herdr
                .herdr(
                    "agent.read",
                    json!({"target":agent.pane_id,"source":"visible","format":"text","lines":40}),
                )
                .await?;
            herdr::get(&self.herdr, agent).await?;
            if let Some(text) = response["result"]["read"]["text"].as_str() {
                self.state.enqueue(
                    &group,
                    &format!("Agent needs input (terminal context):\n{text}"),
                );
            }
            self.state
                .mappings
                .get_mut(key)
                .unwrap()
                .blocked_context_seq = Some(agent.state_change_seq);
        }
        if agent.settled()
            && (self.state.mappings[key].completion_pending
                || now().saturating_sub(self.state.mappings[key].last_export_at) >= 30)
        {
            self.collect_finals(key, agent, &group).await?;
            if self.state.mappings[key].completion_pending {
                self.state.mappings.get_mut(key).unwrap().pending_prompt = false;
                self.state.mappings.get_mut(key).unwrap().completion_pending = false;
            }
        }
        let m = self.state.mappings.get_mut(key).unwrap();
        m.last_agent_status = agent.agent_status.clone();
        m.last_state_change_seq = agent.state_change_seq;
        m.active = true;
        m.agent_record = Some(agent.clone());
        self.save()?;
        Ok(())
    }

    pub async fn inbound(&mut self, event: &Value) -> Result<()> {
        ensure!(
            event["marmot_agent_control"] == PROTOCOL,
            "MDK event protocol mismatch"
        );
        if event["type"] != "inbound_message"
            || event["account_id_hex"] != self.config.account_id_hex
        {
            return Ok(());
        }
        let msg = &event["message"];
        if msg["sender"]["account_id_hex"] != self.phone || msg["sender"]["is_self"] == true {
            return Ok(());
        }
        let Some(group) = event["group_id_hex"].as_str() else {
            return Ok(());
        };
        let Some(key) = self
            .state
            .mappings
            .iter()
            .find(|(_, m)| m.group_id_hex.as_deref() == Some(group))
            .map(|(k, _)| k.clone())
        else {
            return Ok(());
        };
        let Some(id) = msg["message_id_hex"].as_str() else {
            return Ok(());
        };
        ensure!(
            self.herdr_available,
            "HERDR unavailable; inbound message deferred until recovery"
        );
        let mapping = &self.state.mappings[&key];
        if mapping.seen_messages.iter().any(|seen| seen == id) {
            return Ok(());
        }
        ensure!(
            self.agents.contains_key(&key) || !mapping.active,
            "Session identity is uncertain; phone input deferred"
        );
        let text = msg["text"].as_str().unwrap_or("");
        if text != "/status"
            && !text.trim().is_empty()
            && msg["text_truncated"] != true
            && msg["attachments_truncated"] != true
            && msg["media"].as_array().is_none_or(|a| a.is_empty())
            && self.agents.get(&key).is_some_and(Agent::settled)
            && !self.state.mappings[&key].pending_prompt
        {
            let agent = self.agents[&key].clone();
            match herdr::get(&self.herdr, &agent).await {
                Ok(current) if current.settled() => {}
                Err(error) if error.downcast_ref::<crate::wire::NotSubmitted>().is_some() => {
                    self.herdr_available = false;
                    return Err(error);
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Agent readiness changed; recover message after a fresh snapshot"
                    ));
                }
            }
        }
        // Persist consumption before terminal input: uncertain prompts must not run twice.
        let m = self.state.mappings.get_mut(&key).unwrap();
        m.seen_messages.push(id.into());
        self.save()?;
        let response = if text == "/status" {
            Some(format!(
                "HERDR session: {}. State: {}.",
                if self.agents.contains_key(&key) {
                    "connected"
                } else {
                    "ended"
                },
                self.state.mappings[&key].last_agent_status
            ))
        } else if msg["text_truncated"] == true || msg["attachments_truncated"] == true {
            Some("This offline message was truncated by MDK. No prompt was sent. Send a shorter text message.".into())
        } else if !msg["media"].as_array().is_none_or(|a| a.is_empty()) {
            Some("Attachments are not supported. Send a text message.".into())
        } else if let Some(agent) = self.agents.get(&key).cloned() {
            if self.state.mappings[&key].pending_prompt || !agent.settled() {
                Some("The agent is busy or blocked. Wait for it to become idle before sending another prompt.".into())
            } else if text.trim().is_empty() {
                Some("Send a non-empty text message.".into())
            } else {
                self.state.mappings.get_mut(&key).unwrap().pending_prompt = true;
                self.save()?;
                match herdr::prompt(&self.herdr, &agent, text).await {
                    Ok(_) => None,
                    Err(error) => {
                        if error.downcast_ref::<crate::wire::NotSubmitted>().is_some() {
                            let m = self.state.mappings.get_mut(&key).unwrap();
                            m.pending_prompt = false;
                            m.seen_messages.retain(|seen| seen != id);
                            self.herdr_available = false;
                            self.save()?;
                            return Err(error);
                        }
                        Some("The prompt could not be confirmed. It will not be retried. Inspect the agent before submitting again. The local clear-prompt command can release an uncertain prompt.".into())
                    }
                }
            }
        } else {
            Some("This session has ended. No prompt was sent.".into())
        };
        if let Some(text) = response {
            self.state.enqueue(group, &text);
        }
        self.save()
    }

    pub async fn recover_inbound(&mut self) -> Result<()> {
        let groups: Vec<_> = self
            .state
            .mappings
            .iter()
            .filter_map(|(key, m)| m.group_id_hex.as_ref().map(|g| (key.clone(), g.clone())))
            .collect();
        let mut failed = false;
        for (_, group) in groups {
            if self.recover_group(&group).await.is_err() {
                failed = true;
            }
        }
        ensure!(!failed, "Some groups still need inbound recovery");
        Ok(())
    }

    async fn recover_group(&mut self, group: &str) -> Result<()> {
        // Sender timestamps are not ingestion cursors. Rescan retained history and deduplicate
        // by durable message ID so delayed relay deliveries cannot fall behind a watermark.
        let mut before: Option<Value> = None;
        let mut pages = Vec::new();
        loop {
            let mut req = json!({"type":"timeline_list","account_id_hex":self.config.account_id_hex,"group_id_hex":group,"limit":50});
            if let Some(cursor) = &before {
                req["before"] = cursor.clone();
            }
            let page = self.marmot.marmot(req).await?;
            ensure!(page["type"] == "timeline_page", "Invalid timeline response");
            let rows = page["messages"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Missing timeline messages"))?
                .clone();
            if rows.is_empty() {
                break;
            }
            let cursor = json!({"recorded_at":rows[0]["recorded_at"], "message_id_hex":rows[0]["message_id_hex"]});
            ensure!(
                before.as_ref() != Some(&cursor),
                "Timeline cursor did not advance"
            );
            before = Some(cursor);
            pages.push(rows);
            if page["has_more_before"] != true {
                break;
            }
        }
        for rows in pages.into_iter().rev() {
            for row in rows {
                if row["availability"] == "available" && row["kind"] == 9 {
                    self.inbound(&json!({"marmot_agent_control":PROTOCOL,"type":"inbound_message","account_id_hex":self.config.account_id_hex,"group_id_hex":group,"message":{"message_id_hex":row["message_id_hex"],"sender":row["sender"],"text":row["text"],"text_truncated":row["text_truncated"],"attachments_truncated":row["attachments_truncated"],"recorded_at":row["recorded_at"],"media":row["attachments"]}})).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn rename_groups(&self) -> Result<()> {
        let live: BTreeMap<_, _> = herdr::snapshot(&self.herdr)
            .await?
            .into_iter()
            .map(|a| (a.key(), a))
            .collect();
        for (key, mapping) in &self.state.mappings {
            let Some(group) = &mapping.group_id_hex else {
                continue;
            };
            let Some(agent) = live.get(key).or(mapping.agent_record.as_ref()) else {
                continue;
            };
            let name = agent.group_name();
            let info = self.marmot.marmot(json!({"type":"group_info","account_id_hex":self.config.account_id_hex,"group_id_hex":group})).await?;
            ensure!(
                info["type"] == "group_info",
                "Unexpected MDK group response"
            );
            if info["subject"] == name {
                continue;
            }
            // Profile updates have no idempotency key. Never retry an uncertain mutation.
            let result = self.marmot.marmot(json!({"type":"group_profile_update","account_id_hex":self.config.account_id_hex,"group_id_hex":group,"name":name})).await?;
            ensure!(
                result["type"] == "group_profile_updated"
                    && result["message_ids_hex"]
                        .as_array()
                        .is_some_and(|ids| !ids.is_empty()),
                "MDK did not confirm the group rename"
            );
            let info = self.marmot.marmot(json!({"type":"group_info","account_id_hex":self.config.account_id_hex,"group_id_hex":group})).await?;
            ensure!(
                info["type"] == "group_info" && info["subject"] == name,
                "Renamed group title could not be verified"
            );
            println!("Renamed: {name}");
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(256);
        let marmot = self.marmot.clone();
        let account = self.config.account_id_hex.clone();
        let subscriber = tokio::spawn(async move {
            loop {
                if let Ok(mut reader) = marmot.subscribe(json!({"type":"subscribe_inbound","account_id_hex":account,"group_id_hex":null}), true).await {
                    if tx.send(json!({"type":"reconcile_inbound"})).await.is_err() { return; }
                    while let Ok(event) = read_line(&mut reader).await {
                        if tx.send(event).await.is_err() { return; }
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        // Authoritative snapshots avoid relying on HERDR's non-durable, differently shaped event payloads.
        // State-change sequence and stored assistant message IDs also catch work completed between snapshots.
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut recovery_due = true;
        let mut recovery_tick = tokio::time::interval(Duration::from_secs(60));
        recovery_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => { subscriber.abort(); return Ok(()); },
                _ = tick.tick() => {
                    match herdr::snapshot(&self.herdr).await {
                        Ok(agents) => if self.reconcile(agents).await.is_err() { eprintln!("Session reconciliation failed; will retry."); },
                        Err(_) => { self.herdr_available = false; recovery_due = true; eprintln!("HERDR is unavailable; waiting for reconnection."); }
                    }
                },
                Some(event) = rx.recv() => {
                    if event["type"] == "reconcile_inbound" || event["type"] == "resync_required" {
                        // Refresh current agent identities before recovering any queued phone prompts.
                        recovery_due = true;
                    } else if self.inbound(&event).await.is_err() { recovery_due = true; eprintln!("Inbound message deferred; will reconcile history."); }
                },
                _ = recovery_tick.tick() => { recovery_due = true; }
            }
            if recovery_due && self.herdr_available {
                recovery_due = self.recover_inbound().await.is_err();
            }
            if self.flush().await.is_err() {
                eprintln!("MDK send unavailable; private outbox retained for retry.");
            }
        }
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
