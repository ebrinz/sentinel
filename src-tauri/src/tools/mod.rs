pub mod auto_mechanic;
pub mod mac_troubleshoot;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Describes a single tool that a module exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub data: Value,
    pub error: Option<String>,
}

/// A pluggable module that exposes a set of callable tools.
pub trait ToolModule: Send + Sync {
    /// Human-readable module name.
    fn name(&self) -> &str;

    /// One-line description of what this module does.
    fn description(&self) -> &str;

    /// Return the full list of tools this module provides.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// Execute a named tool with the given JSON arguments.
    fn execute(&self, tool_name: &str, args: Value) -> ToolResult;
}

/// Registry that holds N tool modules and dispatches by tool name in O(1).
pub struct ModuleRegistry {
    modules: Vec<Arc<dyn ToolModule>>,
    tool_index: HashMap<String, usize>, // tool_name → index into modules
}

impl ModuleRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            tool_index: HashMap::new(),
        }
    }

    /// Register a module. Rejects name collisions with a descriptive error.
    pub fn register(&mut self, module: Arc<dyn ToolModule>) -> Result<(), String> {
        let idx = self.modules.len();
        for tool in module.tools() {
            if let Some(&existing_idx) = self.tool_index.get(&tool.name) {
                return Err(format!(
                    "Tool '{}' from module '{}' collides with module '{}'",
                    tool.name,
                    module.name(),
                    self.modules[existing_idx].name()
                ));
            }
            self.tool_index.insert(tool.name, idx);
        }
        self.modules.push(module);
        Ok(())
    }

    /// Return all tool definitions across all registered modules.
    pub fn all_tools(&self) -> Vec<ToolDefinition> {
        self.modules.iter().flat_map(|m| m.tools()).collect()
    }

    /// Execute a tool by name, dispatching to the owning module via the index.
    pub fn execute(&self, tool_name: &str, args: Value) -> ToolResult {
        match self.tool_index.get(tool_name) {
            Some(&idx) => self.modules[idx].execute(tool_name, args),
            None => ToolResult {
                success: false,
                data: Value::Null,
                error: Some(format!("Unknown tool: {}", tool_name)),
            },
        }
    }

    /// Check if a tool name is registered.
    pub fn has_tool(&self, tool_name: &str) -> bool {
        self.tool_index.contains_key(tool_name)
    }

    /// Return tool definitions for a specific module only.
    pub fn module_tools(&self, module_name: &str) -> Vec<ToolDefinition> {
        self.modules
            .iter()
            .filter(|m| m.name() == module_name)
            .flat_map(|m| m.tools())
            .collect()
    }

    /// Check if a tool belongs to a specific module.
    pub fn tool_belongs_to_module(&self, tool_name: &str, module_name: &str) -> bool {
        match self.tool_index.get(tool_name) {
            Some(&idx) => self.modules[idx].name() == module_name,
            None => false,
        }
    }

    /// Return the names of all registered modules (for status/debug).
    pub fn module_names(&self) -> Vec<&str> {
        self.modules.iter().map(|m| m.name()).collect()
    }

    /// Return structured info about each registered module.
    pub fn modules_info(&self) -> Vec<ModuleInfo> {
        self.modules
            .iter()
            .map(|m| ModuleInfo {
                name: m.name().to_string(),
                description: m.description().to_string(),
                tool_count: m.tools().len(),
                tool_names: m.tools().iter().map(|t| t.name.clone()).collect(),
            })
            .collect()
    }
}

/// Summary info about a registered module, suitable for sending to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleInfo {
    pub name: String,
    pub description: String,
    pub tool_count: usize,
    pub tool_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::mac_troubleshoot::MacTroubleshootModule;
    use serde_json::json;

    #[test]
    fn test_register_and_dispatch() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Arc::new(MacTroubleshootModule::new()))
            .unwrap();

        assert!(registry.has_tool("monitor_cpu"));
        assert!(registry.has_tool("monitor_memory"));

        let result = registry.execute("monitor_cpu", json!({}));
        assert!(result.success);
    }

    #[test]
    fn test_name_collision_rejected() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Arc::new(MacTroubleshootModule::new()))
            .unwrap();

        // Registering the same module again should fail on name collision
        let err = registry
            .register(Arc::new(MacTroubleshootModule::new()))
            .unwrap_err();
        assert!(err.contains("collides with"));
    }

    #[test]
    fn test_unknown_tool_error() {
        let registry = ModuleRegistry::new();
        let result = registry.execute("nonexistent_tool", json!({}));
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unknown tool"));
    }

    #[test]
    fn test_all_tools_and_module_names() {
        let mut registry = ModuleRegistry::new();
        registry
            .register(Arc::new(MacTroubleshootModule::new()))
            .unwrap();

        let tools = registry.all_tools();
        assert!(!tools.is_empty());

        let names = registry.module_names();
        assert_eq!(names, vec!["mac_troubleshoot"]);
    }
}
