pub mod skill_tool;

use synthia_core::Error;

use crate::types::Skill;

pub struct BuiltinLoader;

impl BuiltinLoader {
    pub fn load_builtins() -> Result<Vec<Skill>, Error> {
        Ok(Vec::new())
    }

    pub fn builtin_skill_names() -> Vec<&'static str> {
        Vec::new()
    }
}
