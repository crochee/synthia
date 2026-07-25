use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use synthia_core::{
    Error,
    registry::Registry,
    tool::extension_registry::CommandStore,
};

use super::{
    definition::CommandDefinition,
    filter::CommandFilter,
    user_command_loader::load_commands_from_directory,
};
use crate::{parser::parse_command, traits::CommandHandler, types::*};

pub struct CommandRegistry {
    commands: Arc<RwLock<HashMap<String, Arc<dyn CommandHandler>>>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_builtins(&self) {
        use crate::builtin::{
            clear::ClearCommand,
            help::HelpCommand,
            model::ModelCommand,
            session::SessionCommand,
            skill::SkillCommand,
            todo::TodoCommand,
        };

        self.register_handler(Arc::new(HelpCommand));
        self.register_handler(Arc::new(ClearCommand));
        self.register_handler(Arc::new(ModelCommand));
        self.register_handler(Arc::new(SessionCommand));
        self.register_handler(Arc::new(SkillCommand::new()));
        self.register_handler(Arc::new(TodoCommand::new()));
    }

    pub fn load_user_commands(&self, workspace_root: &Path) {
        let commands_dir = workspace_root.join(".agents").join("commands");
        if !commands_dir.exists() {
            return;
        }

        let commands = load_commands_from_directory(&commands_dir);
        let mut map = self.commands.write().expect("RwLock poisoned");
        for cmd in commands {
            let name = cmd.name().to_string();
            map.insert(name, Arc::new(cmd));
        }
    }

    pub fn register_handler(&self, handler: Arc<dyn CommandHandler>) {
        let name = handler.name().to_string();
        self.commands
            .write()
            .expect("RwLock poisoned")
            .insert(name, handler);
    }

    pub async fn dispatch(
        &self,
        input: &str,
        ctx: &CommandContext,
    ) -> Result<Option<CommandResult>, Error> {
        let (name, args) = match parse_command(input) {
            Some(pair) => pair,
            None => return Ok(None),
        };

        let handler = {
            let map = self.commands.read().expect("RwLock poisoned");
            map.get(&name).cloned()
        };
        if let Some(handler) = handler {
            let result = handler.execute(&args, ctx).await?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    pub fn len(&self) -> usize {
        self.commands.read().expect("RwLock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.read().expect("RwLock poisoned").is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands
            .read()
            .expect("RwLock poisoned")
            .contains_key(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Registry<CommandDefinition> for CommandRegistry {
    type Filter = CommandFilter;

    async fn register(
        &self,
        item: CommandDefinition,
    ) -> Result<CommandDefinition, Error> {
        let name = item.name.clone();
        let map = self.commands.read().expect("RwLock poisoned");
        if map.contains_key(&name) {
            return Err(Error::AlreadyExists(name));
        }
        Err(Error::Internal(
            "CommandRegistry does not support direct registration without CommandHandler"
                .to_string(),
        ))
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let removed =
            self.commands.write().expect("RwLock poisoned").remove(name);
        match removed {
            Some(_) => Ok(()),
            None => Err(Error::NotFound(name.to_string())),
        }
    }

    async fn get(
        &self,
        name: &str,
    ) -> Result<Option<CommandDefinition>, Error> {
        let map = self.commands.read().expect("RwLock poisoned");
        Ok(map.get(name).map(|h| CommandDefinition {
            name: h.name().to_string(),
            description: h.description().to_string(),
        }))
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<CommandDefinition>, Error> {
        let map = self.commands.read().expect("RwLock poisoned");
        let commands: Vec<CommandDefinition> = map
            .values()
            .map(|h| CommandDefinition {
                name: h.name().to_string(),
                description: h.description().to_string(),
            })
            .collect();
        match filter {
            Some(f) => Ok(commands
                .into_iter()
                .filter(|c| f.matches_command(c))
                .collect()),
            None => Ok(commands),
        }
    }
}

impl CommandStore for CommandRegistry {
    fn command_count(&self) -> usize {
        self.len()
    }

    fn contains_command(&self, name: &str) -> bool {
        self.contains(name)
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl Clone for CommandRegistry {
    fn clone(&self) -> Self {
        // Share the internal command map via Arc. This is safe because
        // CommandRegistry uses interior mutability (RwLock), so clones
        // share the same underlying data.
        Self {
            commands: Arc::clone(&self.commands),
        }
    }
}
