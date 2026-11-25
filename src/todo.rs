use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoItem {
    pub id: u64,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TodoStore {
    pub items: Vec<TodoItem>,
    next_id: u64,
}

impl TodoStore {
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "liebert-dynamic-island")
            .map(|dirs| dirs.config_dir().join("todos.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Self {
        let mut store: Self = toml::from_str(raw).unwrap_or_default();
        let max_id = store.items.iter().map(|i| i.id).max().unwrap_or(0);
        if store.next_id <= max_id {
            store.next_id = max_id + 1;
        }
        store
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, encoded)
    }

    pub fn add(&mut self, text: &str) -> Option<u64> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        self.next_id = self.next_id.max(1);
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(TodoItem {
            id,
            text: trimmed.to_owned(),
            done: false,
        });
        Some(id)
    }

    pub fn toggle(&mut self, id: u64) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.done = !item.done;
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.items.retain(|i| i.id != id);
    }

    pub fn clear_done(&mut self) {
        self.items.retain(|i| !i.done);
    }

    pub fn pending_count(&self) -> usize {
        self.items.iter().filter(|i| !i.done).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_toggle_remove_and_clear_done() {
        let mut store = TodoStore::default();
        assert_eq!(store.add("   "), None);
        let id1 = store.add("  Reply email  ").expect("valid todo");
        let id2 = store.add("Review PR").expect("valid todo");
        assert_eq!(store.pending_count(), 2);

        store.toggle(id1);
        assert_eq!(store.pending_count(), 1);

        store.clear_done();
        assert_eq!(store.items.len(), 1);
        assert_eq!(store.items[0].id, id2);

        store.remove(id2);
        assert!(store.items.is_empty());
    }

    #[test]
    fn toml_round_trip() {
        let mut store = TodoStore::default();
        store.add("Build island");
        let raw = toml::to_string_pretty(&store).expect("serialize");
        let parsed = TodoStore::from_toml_str(&raw);
        assert_eq!(parsed, store);
    }
}
