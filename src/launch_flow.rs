//! Chat-bound project setup menus; active menu choices take priority over agent decisions.
use crate::{
    bridge::{Bridge, now},
    catalogue::{self, Model},
    herdr::{self, Agent},
    launch,
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    New,
    Fresh(Agent),
    SwitchModel(Agent),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Workspace,
    Folder,
    FolderList,
    Agent,
    Model,
    ModelGroup,
    Variant,
    ModelConfirm,
    Applying,
    Applied,
    Uncertain,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub code: String,
    pub mode: Mode,
    pub step: Step,
    pub directory: Option<PathBuf>,
    pub kind: Option<String>,
    pub agents: Vec<String>,
    pub models: Vec<Model>,
    pub selected: Option<Model>,
    pub page: usize,
    pub expires_at: u64,
    #[serde(default)]
    pub workspaces: Vec<herdr::Workspace>,
    #[serde(default)]
    pub workspace_list_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_workspace: Option<herdr::Workspace>,
    #[serde(default)]
    pub model_groups: Vec<ModelGroup>,
    #[serde(default)]
    pub model_group: Option<usize>,
    #[serde(default)]
    pub visible_models: Vec<usize>,
    #[serde(default)]
    pub folder_choices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelGroup {
    pub label: String,
    pub models: Vec<usize>,
}

fn code() -> String {
    format!("p{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
}
impl Draft {
    fn new(mode: Mode, step: Step) -> Self {
        Self {
            code: code(),
            mode,
            step,
            directory: None,
            kind: None,
            agents: vec![],
            models: vec![],
            selected: None,
            page: 0,
            expires_at: now().saturating_add(600),
            workspaces: vec![],
            workspace_list_available: false,
            target_workspace: None,
            model_groups: vec![],
            model_group: None,
            visible_models: vec![],
            folder_choices: vec![],
        }
    }
    fn advance(&mut self, step: Step) {
        self.step = step;
        self.code = code();
        self.expires_at = now().saturating_add(600);
    }
    fn blocks_replacement(&self) -> bool {
        matches!(self.step, Step::Applying | Step::Uncertain)
    }
    fn begin_models(&mut self) {
        let mut groups = std::collections::BTreeMap::new();
        for (index, model) in self
            .models
            .iter()
            .enumerate()
            .filter(|(_, model)| !model.id.is_empty() && model.variant.is_none())
        {
            let (rank, label) = if model.free {
                (2, "Free models".to_string())
            } else {
                match model.provider.as_deref() {
                    Some("openai") => (0, "OpenAI / Codex models".into()),
                    Some("anthropic") => (1, "Claude models".into()),
                    Some(provider) => (3, format!("{provider} models")),
                    None => (
                        3,
                        format!(
                            "{} models",
                            catalogue::agent_name(self.kind.as_deref().unwrap_or("agent"))
                        ),
                    ),
                }
            };
            groups
                .entry((rank, label))
                .or_insert_with(Vec::new)
                .push(index);
        }
        self.visible_models.clear();
        self.model_group = None;
        if !groups.is_empty() {
            self.model_groups = groups
                .into_iter()
                .map(|((_, label), models)| ModelGroup { label, models })
                .collect();
            self.step = Step::ModelGroup;
        } else {
            self.model_groups.clear();
            self.step = Step::Model;
        }
    }
    fn model_options(&self) -> Vec<usize> {
        if self.visible_models.is_empty() {
            (0..self.models.len()).collect()
        } else {
            self.visible_models.clone()
        }
    }
    fn heading(&self, step: usize, title: &str) -> String {
        if self.mode == Mode::New {
            format!("[{step}/4] {title}")
        } else {
            title.to_string()
        }
    }
    fn prompt(&self) -> String {
        let mut text=match self.step {
            Step::Workspace=>{
                let mut text=self.heading(1,"Choose a herdr workspace:");
                if self.workspaces.len()>10 {text.push_str(&format!("\npage {} of {}",self.page+1,self.workspaces.len().div_ceil(10)));}
                for (i,workspace) in self.workspaces.iter().enumerate().skip(self.page*10).take(10) {text.push_str(&format!("\n{}. {}",i+1,workspace.display()));}
                text.push_str(&format!("\n\n**{}. Create new workspace**\n\n**0. Cancel**",self.workspaces.len()+1));
                if !self.workspace_list_available {text.push_str("\nExisting workspaces are unavailable. Send /new to retry the list, or choose new.");}
                text.push_str("\nReply with a listed number.");
                if (self.page+1)*10<self.workspaces.len() {text.push_str("\nNext page: next");}
                if self.page>0 {text.push_str("\nPrevious page: previous");}
                text
            }
            Step::Folder=>format!("{}\nReply with your chosen folder name, or ls to list existing folders.\n\n**0. Cancel**\nNothing will be created yet.",self.heading(2,"Folder name:")),
            Step::FolderList=>{
                let mut text=self.heading(2,"Choose a folder:");
                if self.folder_choices.is_empty() {
                    text.push_str("\nNo folders found under the launch root. Type a folder name.");
                } else {
                    for (i,name) in self.folder_choices.iter().take(40).enumerate() {
                        text.push_str(&format!("\n{}. {name}",i+1));
                    }
                    if self.folder_choices.len()>40 {
                        text.push_str("\nShowing the first 40; type a name for another.");
                    }
                }
                text.push_str("\n\n**0. Cancel**\nReply with a listed number, a folder name, or ls again.");
                text
            }
            Step::Agent=>{
                let mut text=self.heading(3,"Choose an installed agent:");
                for (i,kind) in self.agents.iter().enumerate() {text.push_str(&format!("\n{}. {}",i+1,catalogue::agent_name(kind)));}
                text.push_str("\n\n**0. Cancel**\nReply with a listed number.");text
            }
            Step::ModelGroup=>{
                let mut text=self.heading(4,&format!("Choose a model for {}:",catalogue::agent_name(self.kind.as_deref().unwrap_or("agent"))));
                let offset=usize::from(self.models.first().is_some_and(|m|m.id.is_empty()));
                text.push_str(if offset==1 {"\nUse the backend default, or view models from a provider."}else{"\nView models from a provider."});
                if offset==1 {text.push_str("\n1. Backend default");}
                for (i,group) in self.model_groups.iter().enumerate() {text.push_str(&format!("\n{}. View {}",i+offset+1,group.label));}
                text.push_str("\n\n**0. Cancel**\nReply with a listed number.");text
            }
            Step::Model|Step::Variant=>{
                let mut text=self.heading(4,&if self.step==Step::Variant {
                    format!("Choose a variant for {}:",self.selected.as_ref().unwrap().label)
                } else {format!("Choose a model for {}:",catalogue::agent_name(self.kind.as_deref().unwrap_or("agent")))});
                let options=self.model_options();
                let paged=self.model_groups.is_empty(); // Keep numbering in older saved flat menus unchanged.
                if paged && options.len()>10 {text.push_str(&format!("\npage {} of {}",self.page+1,options.len().div_ceil(10)));}
                for (i,index) in options.iter().enumerate().skip(if paged {self.page*10}else{0}).take(if paged {10}else{usize::MAX}) {
                    let model=&self.models[*index];
                    let label=if self.step==Step::Variant {model.variant.as_deref().unwrap_or("Model default").to_string()} else {model.display()};
                    text.push_str(&format!("\n{}. {label}",i+1));
                }
                text.push_str("\n\n**0. Cancel**\nReply with a listed number.");
                if !self.model_groups.is_empty() {text.push_str("\nBack: back");}
                if paged && (self.page+1)*10<options.len() {text.push_str("\nNext page: next");}
                if paged && self.page>0 {text.push_str("\nPrevious page: previous");}
                text
            }
            Step::ModelConfirm=>format!("Change this OpenCode session's model to {}?\nHistory and chat stay unchanged. Applies to subsequent turns only.\n/model confirm\n\n**0. Cancel**",self.selected.as_ref().unwrap().display()),
            Step::Applying|Step::Uncertain=>"Model selection was not confirmed. Inspect OpenCode locally. No automatic retry will run.".into(),
            Step::Applied=>"Model selection was confirmed. No change was repeated.".into(),
        };
        if matches!(self.mode, Mode::Fresh(_)) {
            text.push_str("\nThis starts a fresh session in a new tab and separate chat. Old history and queued work stay in the old chat.");
        }
        if !self.blocks_replacement() {
            text.push_str("\nMenus expire after 10 minutes.");
        }
        text
    }
}

fn folder_choices(root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let name = entry.file_name().to_str()?.to_owned();
            // Same rule as a typed folder name, so every listed entry can be picked.
            let valid = !name.is_empty()
                && name.len() <= 48
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
            valid.then_some(name)
        })
        .collect();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    // One past the 40 shown, so the menu knows when to say the list is cut.
    names.truncate(41);
    names
}

pub fn is_command(text: &str) -> bool {
    text.split_whitespace().next().is_some_and(|word| {
        matches!(
            word.to_ascii_lowercase().as_str(),
            "/new" | "/agent" | "/model"
        )
    })
}

/// Resolve only selection replies against the current chat's menu, never a confirmation.
pub fn short_reply(draft: &Draft, text: &str) -> Option<String> {
    if text.trim() == "0" {
        return Some("/new cancel".into());
    }
    let answer = text.trim().to_ascii_lowercase();
    let number = !answer.is_empty() && answer.bytes().all(|b| b.is_ascii_digit());
    let page = matches!(answer.as_str(), "next" | "previous");
    match draft.step {
        Step::Folder if answer == "ls" => Some(format!("/new list {}", draft.code)),
        Step::Folder if !text.trim().is_empty() && !text.trim().starts_with('/') => {
            Some(format!("/new folder {} {}", draft.code, text.trim()))
        }
        Step::FolderList if answer == "ls" => Some(format!("/new list {}", draft.code)),
        Step::FolderList if number => Some(format!("/new pick {} {answer}", draft.code)),
        Step::FolderList if !text.trim().is_empty() && !text.trim().starts_with('/') => {
            Some(format!("/new folder {} {}", draft.code, text.trim()))
        }
        Step::Workspace if number || page || answer == "new" => {
            Some(format!("/new workspace {} {answer}", draft.code))
        }
        Step::Agent if number => Some(format!("/agent {} {answer}", draft.code)),
        Step::Model | Step::ModelGroup | Step::Variant if number || page || answer == "back" => {
            Some(format!("/model {} {answer}", draft.code))
        }
        _ => None,
    }
}

fn expand_reply(bridge: &Bridge, key: &str, text: &str) -> String {
    let args: Vec<_> = text.split_whitespace().collect();
    let mapping = &bridge.state.mappings[key];
    if text.trim() == "0"
        && mapping
            .launch
            .as_ref()
            .is_some_and(|request| request.phase == launch::Phase::Awaiting)
    {
        return "/new cancel".into();
    }
    if let Some(draft) = &mapping.launch_draft {
        if let Some(command) = short_reply(draft, text) {
            return command;
        }
        match args.as_slice() {
            [command, field, value]
                if command.eq_ignore_ascii_case("/new")
                    && matches!(field.to_ascii_lowercase().as_str(), "folder" | "workspace") =>
            {
                return format!("/new {field} {} {value}", draft.code);
            }
            [command, value]
                if (command.eq_ignore_ascii_case("/agent") && draft.step == Step::Agent)
                    || (command.eq_ignore_ascii_case("/model")
                        && matches!(
                            draft.step,
                            Step::Model | Step::ModelGroup | Step::Variant
                        )) =>
            {
                return format!("{command} {} {value}", draft.code);
            }
            [command, confirm]
                if command.eq_ignore_ascii_case("/model")
                    && confirm.eq_ignore_ascii_case("confirm")
                    && draft.step == Step::ModelConfirm =>
            {
                return format!("/model confirm {}", draft.code);
            }
            _ => {}
        }
    }
    if mapping.launch_draft.is_none()
        && let Some(request) = &mapping.launch
        && request.phase == launch::Phase::Awaiting
        && text.trim() == request.confirmation_name()
    {
        return format!("/new confirm {}", request.code);
    }
    if let [command, confirm] = args.as_slice()
        && command.eq_ignore_ascii_case("/new")
        && confirm.eq_ignore_ascii_case("confirm")
        && let Some(request) = &mapping.launch
    {
        return format!("/new confirm {}", request.code);
    }
    text.to_string()
}

fn persist(
    bridge: &mut Bridge,
    key: &str,
    group: &str,
    id: &str,
    draft: Option<Draft>,
    text: &str,
    invalidate_launch: bool,
) -> Result<()> {
    let before = bridge.state.clone();
    if invalidate_launch
        && let Some(request) = bridge.state.mappings.get_mut(key).unwrap().launch.as_mut()
        && request.phase == launch::Phase::Awaiting
    {
        request.phase = launch::Phase::Cancelled;
    }
    bridge.state.mappings.get_mut(key).unwrap().launch_draft = draft;
    if let Err(error) = launch::reply(bridge, key, group, id, text) {
        bridge.state = before;
        return Err(error);
    }
    Ok(())
}

fn show(bridge: &mut Bridge, key: &str, group: &str, id: &str, draft: Draft) -> Result<()> {
    let text = draft.prompt();
    persist(bridge, key, group, id, Some(draft), &text, true)
}

async fn live(bridge: &Bridge, key: &str) -> Result<Agent> {
    let record = bridge.state.mappings[key]
        .agent_record
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("This chat has no mapped project. Start with /new."))?;
    let current = herdr::get(&bridge.herdr, record).await?;
    ensure!(current.key() == key, "The mapped session changed");
    ensure!(
        !current.cwd.is_empty() && !current.workspace_id.is_empty(),
        "Project location is unavailable"
    );
    Ok(current)
}

fn request(bridge: &Bridge, draft: &Draft) -> Result<launch::Request> {
    let directory = draft
        .directory
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Choose a folder first"))?;
    let kind = draft
        .kind
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Choose an agent first"))?;
    let name = directory
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid folder name"))?;
    let label: String = name
        .chars()
        .map(|c| if c == '.' { '-' } else { c })
        .take(48)
        .collect();
    let mut request = launch::proposal(
        &format!("/new {label} {} {kind}", directory.display()),
        &bridge.config.launch_root,
        &PathBuf::from(std::env::var_os("HOME").unwrap_or_default()),
        now(),
    )?;
    request.model = draft.selected.clone();
    request.target_workspace = draft.target_workspace.clone();
    if let Mode::Fresh(source) = &draft.mode {
        request.source_agent = Some(source.clone());
    }
    Ok(request)
}

pub async fn handle(
    bridge: &mut Bridge,
    key: &str,
    group: &str,
    id: &str,
    text: &str,
    plain: bool,
) -> Result<()> {
    if !plain {
        return launch::reply(
            bridge,
            key,
            group,
            id,
            "Send project controls as complete standalone text, without attachments or a voice transcript.",
        );
    }
    if bridge.state.mappings[key].launch_draft.is_none()
        && let Some(request) = &bridge.state.mappings[key].launch
        && request.phase == launch::Phase::Awaiting
        && !text.trim().starts_with('/')
        && text.trim() != "0"
    {
        let word = text.trim().to_ascii_lowercase();
        if now() >= request.expires_at || matches!(word.as_str(), "confirm" | "yes" | "y") {
            let command = format!("/new confirm {}", request.code);
            return launch::handle(bridge, key, group, id, &command, true).await;
        }
        if text.trim() != request.confirmation_name() {
            return launch::reply(
                bridge,
                key,
                group,
                id,
                &format!(
                    "That name does not match the pending workspace. Reply confirm, yes or y (or type: {}), or 0 to cancel.",
                    request.confirmation_name()
                ),
            );
        }
    }
    let text = expand_reply(bridge, key, text);
    match route(bridge, key, group, id, &text).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let draft = bridge.state.mappings[key].launch_draft.clone();
            persist(
                bridge,
                key,
                group,
                id,
                draft,
                &format!("Project configuration stopped: {error}. No automatic retry will run."),
                false,
            )
        }
    }
}

async fn route(bridge: &mut Bridge, key: &str, group: &str, id: &str, text: &str) -> Result<()> {
    let args: Vec<_> = text.split_whitespace().collect();
    let command = args[0].to_ascii_lowercase();
    let draft = bridge.state.mappings[key].launch_draft.clone();
    // A guided /new menu synthesises /agent and /model replies; only a fresh
    // /agent or /model in the control chat needs a mapped project.
    if bridge.state.mappings[key].control
        && draft.is_none()
        && matches!(command.as_str(), "/agent" | "/model")
    {
        return launch::reply(
            bridge,
            key,
            group,
            id,
            "The control chat is not mapped to a project. Use /new to start a session.",
        );
    }
    if draft.as_ref().is_some_and(Draft::blocks_replacement) {
        return launch::reply(bridge, key, group, id, &draft.unwrap().prompt());
    }
    if args.len() == 2 && command == "/new" && args[1].eq_ignore_ascii_case("cancel") {
        return persist(
            bridge,
            key,
            group,
            id,
            None,
            "Project configuration cancelled. Pending launch confirmations were cancelled. No new action was started.",
            true,
        );
    }
    if command == "/new"
        && args.len() > 1
        && (!matches!(
            args[1].to_ascii_lowercase().as_str(),
            "folder" | "workspace" | "list" | "pick"
        ) || (args.len() == 4
            && args[1].eq_ignore_ascii_case("workspace")
            && draft.as_ref().is_none_or(|draft| args[2] != draft.code)
            && matches!(args[3], "opencode" | "codex" | "claude")))
    {
        return launch::handle(bridge, key, group, id, text, true).await;
    }
    ensure!(
        !bridge.state.mappings[key]
            .launch
            .as_ref()
            .is_some_and(launch::Request::blocks_replacement),
        "A previous launch needs local inspection. Use /new status"
    );
    if args.len() == 1 {
        if command == "/new" {
            let mut draft = Draft::new(Mode::New, Step::Workspace);
            if let Ok(workspaces) = herdr::workspaces(&bridge.herdr).await {
                draft.workspaces = workspaces;
                draft.workspace_list_available = true;
            }
            return show(bridge, key, group, id, draft);
        }
        if let Some(draft) = draft.filter(|d| {
            now() < d.expires_at
                && matches!(
                    (command.as_str(), d.step),
                    ("/agent", Step::Agent)
                        | (
                            "/model",
                            Step::Model | Step::ModelGroup | Step::Variant | Step::ModelConfirm
                        )
                )
        }) {
            return show(bridge, key, group, id, draft);
        }
        let source = live(bridge, key).await?;
        let directory = PathBuf::from(&source.cwd);
        launch::inspect(&bridge.config.launch_root, &directory)?;
        if command == "/agent" {
            let mut draft = Draft::new(Mode::Fresh(source), Step::Agent);
            draft.directory = Some(directory);
            draft.agents = catalogue::installed(&bridge.config);
            ensure!(
                !draft.agents.is_empty(),
                "No supported agents are installed"
            );
            return show(bridge, key, group, id, draft);
        }
        let kind = source
            .agent_session
            .as_ref()
            .map(|s| s.agent.clone())
            .unwrap_or_else(|| source.agent.clone());
        ensure!(
            catalogue::installed(&bridge.config).contains(&kind),
            "The mapped agent is not installed or is unsupported"
        );
        let mode = if kind == "opencode" {
            Mode::SwitchModel(source)
        } else {
            Mode::Fresh(source)
        };
        let mut draft = Draft::new(mode, Step::Model);
        draft.models = catalogue::models(
            &bridge.config,
            &kind,
            &directory,
            !matches!(draft.mode, Mode::SwitchModel(_)),
        )
        .await?;
        draft.kind = Some(kind);
        draft.directory = Some(directory);
        draft.begin_models();
        return show(bridge, key, group, id, draft);
    }
    let mut draft = draft
        .ok_or_else(|| anyhow::anyhow!("No matching menu. Start with /new, /agent, or /model"))?;
    if now() >= draft.expires_at {
        return persist(
            bridge,
            key,
            group,
            id,
            None,
            "Project configuration stopped: This menu expired. Start again with /new, /agent, or /model. No automatic retry will run.",
            true,
        );
    }
    if command == "/new" && args.len() == 4 && args[1].eq_ignore_ascii_case("workspace") {
        ensure!(
            draft.step == Step::Workspace && args[2] == draft.code,
            "Use the current workspace menu code"
        );
        if matches!(args[3].to_ascii_lowercase().as_str(), "next" | "previous") {
            let page = if args[3].eq_ignore_ascii_case("next") {
                draft.page + 1
            } else {
                draft.page.saturating_sub(1)
            };
            ensure!(
                page * 10 < draft.workspaces.len(),
                "That workspace page is unavailable"
            );
            draft.page = page;
            return show(bridge, key, group, id, draft);
        }
        draft.target_workspace = if args[3].eq_ignore_ascii_case("new")
            || args[3].parse::<usize>().ok() == Some(draft.workspaces.len() + 1)
        {
            None
        } else {
            let index = args[3].parse::<usize>()?;
            Some(
                draft
                    .workspaces
                    .get(index.wrapping_sub(1))
                    .ok_or_else(|| anyhow::anyhow!("Choose a listed workspace number or new"))?
                    .clone(),
            )
        };
        draft.page = 0;
        draft.advance(Step::Folder);
        return show(bridge, key, group, id, draft);
    }
    if command == "/new" && args.len() == 4 && args[1].eq_ignore_ascii_case("folder") {
        ensure!(
            matches!(draft.step, Step::Folder | Step::FolderList) && args[2] == draft.code,
            "Use the current folder menu code"
        );
        let proposed = launch::proposal(
            &format!("/new project {} opencode", args[3]),
            &bridge.config.launch_root,
            &PathBuf::from(std::env::var_os("HOME").unwrap_or_default()),
            now(),
        )?;
        draft.directory = Some(proposed.directory);
        draft.agents = catalogue::installed(&bridge.config);
        ensure!(
            !draft.agents.is_empty(),
            "No supported agents are installed"
        );
        draft.advance(Step::Agent);
        return show(bridge, key, group, id, draft);
    }
    if command == "/new" && args.len() == 3 && args[1].eq_ignore_ascii_case("list") {
        ensure!(
            matches!(draft.step, Step::Folder | Step::FolderList) && args[2] == draft.code,
            "Use the current folder menu code"
        );
        draft.folder_choices = folder_choices(&bridge.config.launch_root);
        draft.step = Step::FolderList;
        draft.page = 0;
        return show(bridge, key, group, id, draft);
    }
    if command == "/new" && args.len() == 4 && args[1].eq_ignore_ascii_case("pick") {
        ensure!(
            draft.step == Step::FolderList && args[2] == draft.code,
            "Use the current folder menu code"
        );
        let index = args[3].parse::<usize>()?;
        let name = draft
            .folder_choices
            .iter()
            .take(40)
            .nth(index.wrapping_sub(1))
            .ok_or_else(|| anyhow::anyhow!("Choose a listed folder number"))?
            .clone();
        let proposed = launch::proposal(
            &format!("/new project {name} opencode"),
            &bridge.config.launch_root,
            &PathBuf::from(std::env::var_os("HOME").unwrap_or_default()),
            now(),
        )?;
        draft.directory = Some(proposed.directory);
        draft.agents = catalogue::installed(&bridge.config);
        ensure!(
            !draft.agents.is_empty(),
            "No supported agents are installed"
        );
        draft.advance(Step::Agent);
        return show(bridge, key, group, id, draft);
    }
    if command == "/model" && args.len() == 3 && args[1].eq_ignore_ascii_case("confirm") {
        ensure!(
            draft.step == Step::ModelConfirm && args[2] == draft.code,
            "Use the current model confirmation code"
        );
        return switch_model(bridge, key, group, id, draft).await;
    }
    ensure!(
        args.len() == 3 && args[1] == draft.code,
        "Use the current menu code and a listed option"
    );
    if command == "/agent" {
        ensure!(
            draft.step == Step::Agent,
            "Choose an agent from the current agent menu"
        );
        let index = args[2].parse::<usize>()?;
        let kind = draft
            .agents
            .get(index.wrapping_sub(1))
            .ok_or_else(|| anyhow::anyhow!("Choose a listed agent number"))?
            .clone();
        ensure!(
            catalogue::installed(&bridge.config).contains(&kind),
            "That agent is no longer installed"
        );
        draft.models = catalogue::models(
            &bridge.config,
            &kind,
            draft.directory.as_ref().unwrap(),
            true,
        )
        .await?;
        draft.kind = Some(kind);
        draft.advance(Step::Model);
        draft.begin_models();
        draft.page = 0;
        return show(bridge, key, group, id, draft);
    }
    ensure!(
        command == "/model" && matches!(draft.step, Step::Model | Step::ModelGroup | Step::Variant),
        "Choose a model from the current model menu"
    );
    if args[2].eq_ignore_ascii_case("back") && !draft.model_groups.is_empty() {
        if draft.step == Step::Variant {
            draft.visible_models = draft
                .model_groups
                .get(draft.model_group.unwrap())
                .ok_or_else(|| anyhow::anyhow!("Model group is unavailable"))?
                .models
                .clone();
            draft.advance(Step::Model);
        } else {
            draft.visible_models.clear();
            draft.model_group = None;
            draft.advance(Step::ModelGroup);
        }
        draft.selected = None;
        draft.page = 0;
        return show(bridge, key, group, id, draft);
    }
    if draft.step == Step::ModelGroup {
        let index = args[2].parse::<usize>()?;
        let offset = usize::from(
            draft
                .models
                .first()
                .is_some_and(|model| model.id.is_empty()),
        );
        if index == 1 && offset == 1 {
            draft.selected = Some(draft.models[0].clone());
            return finish_selection(bridge, key, group, id, draft);
        }
        let group_index = index
            .checked_sub(offset + 1)
            .ok_or_else(|| anyhow::anyhow!("Choose a listed model group"))?;
        draft.visible_models = draft
            .model_groups
            .get(group_index)
            .ok_or_else(|| anyhow::anyhow!("Choose a listed model group"))?
            .models
            .clone();
        draft.model_group = Some(group_index);
        draft.selected = None;
        draft.advance(Step::Model);
        draft.page = 0;
        return show(bridge, key, group, id, draft);
    }
    let options = draft.model_options();
    if args[2].eq_ignore_ascii_case("next") || args[2].eq_ignore_ascii_case("previous") {
        ensure!(
            draft.model_groups.is_empty(),
            "All models in this list are already shown. Choose a listed number or back"
        );
        let page = if args[2].eq_ignore_ascii_case("next") {
            draft.page + 1
        } else {
            draft.page.saturating_sub(1)
        };
        ensure!(page * 10 < options.len(), "That page is unavailable");
        draft.page = page;
        return show(bridge, key, group, id, draft);
    }
    let index = args[2].parse::<usize>()?;
    let index = *options
        .get(index.wrapping_sub(1))
        .ok_or_else(|| anyhow::anyhow!("Choose a listed model number"))?;
    let selected = draft.models[index].clone();
    if draft.step == Step::Model && !draft.model_groups.is_empty() {
        let variants: Vec<_> = draft
            .models
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.id == selected.id && m.provider == selected.provider && m.variant.is_some()
            })
            .map(|(i, _)| i)
            .collect();
        if !variants.is_empty() {
            draft.visible_models = std::iter::once(index).chain(variants).collect();
            draft.selected = Some(selected);
            draft.advance(Step::Variant);
            draft.page = 0;
            return show(bridge, key, group, id, draft);
        }
    }
    draft.selected = Some(selected);
    finish_selection(bridge, key, group, id, draft)
}

fn finish_selection(
    bridge: &mut Bridge,
    key: &str,
    group: &str,
    id: &str,
    mut draft: Draft,
) -> Result<()> {
    if matches!(draft.mode, Mode::SwitchModel(_)) {
        draft.advance(Step::ModelConfirm);
        return show(bridge, key, group, id, draft);
    }
    let request = request(bridge, &draft)?;
    let preview = request.preview();
    let before = bridge.state.clone();
    bridge.state.mappings.get_mut(key).unwrap().launch = Some(request);
    if let Err(error) = persist(bridge, key, group, id, None, &preview, false) {
        bridge.state = before;
        return Err(error);
    }
    Ok(())
}

async fn switch_model(
    bridge: &mut Bridge,
    key: &str,
    group: &str,
    id: &str,
    mut draft: Draft,
) -> Result<()> {
    let Mode::SwitchModel(source) = &draft.mode else {
        bail!("This menu is not an in-place model switch")
    };
    let source = source.clone();
    let current = live(bridge, key).await?;
    ensure!(
        current.key() == source.key() && current.cwd == source.cwd,
        "The project or session changed. Open a fresh model menu"
    );
    let mapping = &bridge.state.mappings[key];
    ensure!(
        current.settled()
            && !mapping.pending_prompt
            && mapping.queue_flight.is_none()
            && mapping.prompt_queue.is_empty()
            && mapping.decision.is_none()
            && mapping.permission.is_none(),
        "Wait until this session and its prompt queue are idle, then confirm again"
    );
    let model = draft.selected.as_ref().unwrap().clone();
    ensure!(
        catalogue::models(
            &bridge.config,
            "opencode",
            &PathBuf::from(&current.cwd),
            false
        )
        .await?
        .iter()
        .any(|candidate| candidate.same_reference(&model)),
        "That model is no longer available. Open /model again"
    );
    let session = current
        .agent_session
        .as_ref()
        .filter(|s| s.agent == "opencode" && s.kind == "id")
        .ok_or_else(|| anyhow::anyhow!("Native OpenCode session is unavailable"))?;
    ensure!(
        session.value.starts_with("ses_") && catalogue::valid_id(&session.value),
        "Invalid native session ID"
    );
    let path = format!("/api/session/{}", session.value);
    let before = catalogue::api(&bridge.config, "get", &path, None).await?;
    ensure!(
        before["data"]["id"] == session.value
            && before["data"]["location"]["directory"] == current.cwd,
        "OpenCode session identity or location changed"
    );
    draft.step = Step::Applying;
    let previous = bridge.state.clone();
    bridge.state.mappings.get_mut(key).unwrap().launch_draft = Some(draft.clone());
    bridge
        .state
        .mappings
        .get_mut(key)
        .unwrap()
        .seen_messages
        .push(id.into());
    if let Err(error) = bridge.save() {
        bridge.state = previous;
        return Err(error);
    }
    let response = catalogue::api(
        &bridge.config,
        "post",
        &format!("{path}/model"),
        Some(&json!({"model":model.reference()})),
    )
    .await;
    let readback = catalogue::api(&bridge.config, "get", &path, None).await;
    let confirmed = response.is_ok()
        && readback.as_ref().is_ok_and(|r| {
            r["data"]["id"] == session.value
                && r["data"]["location"]["directory"] == current.cwd
                && r["data"]["model"] == model.reference()
        });
    draft.step = if confirmed {
        Step::Applied
    } else {
        Step::Uncertain
    };
    let message = if confirmed {
        format!(
            "Model changed to {} for subsequent turns. This chat, history, and queue were kept.",
            model.display()
        )
    } else {
        draft.prompt()
    };
    // Keep the non-repeatable phase in memory even if the result cannot be saved.
    bridge.state.mappings.get_mut(key).unwrap().launch_draft = Some(draft);
    launch::reply(bridge, key, group, id, &message)
}

#[cfg(test)]
mod folder_choice_tests {
    use super::{Draft, Mode, Step, folder_choices, short_reply};

    #[test]
    fn ls_lists_only_selectable_directories() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("alpha")).unwrap();
        std::fs::create_dir(directory.path().join("Beta")).unwrap();
        std::fs::create_dir(directory.path().join(".hidden")).unwrap();
        std::fs::create_dir(directory.path().join("dotted.name")).unwrap();
        std::fs::write(directory.path().join("file"), "").unwrap();
        std::os::unix::fs::symlink(
            directory.path().join("alpha"),
            directory.path().join("link"),
        )
        .unwrap();
        assert_eq!(folder_choices(directory.path()), vec!["alpha", "Beta"]);
    }

    #[test]
    fn folder_menu_lists_and_selects_by_number() {
        let mut draft = Draft::new(Mode::New, Step::Folder);
        let code = draft.code.clone();
        assert_eq!(short_reply(&draft, "ls"), Some(format!("/new list {code}")));
        draft.step = Step::FolderList;
        draft.folder_choices = vec!["alpha".into(), "beta".into()];
        assert_eq!(
            short_reply(&draft, "2"),
            Some(format!("/new pick {code} 2"))
        );
        assert_eq!(
            short_reply(&draft, "gamma"),
            Some(format!("/new folder {code} gamma"))
        );
        assert_eq!(short_reply(&draft, "ls"), Some(format!("/new list {code}")));
    }
}
