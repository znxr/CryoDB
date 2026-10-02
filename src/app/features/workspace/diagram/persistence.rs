use super::State;
use crate::model::diagram::{DiagramStore, StoredDiagram};

impl State {
    pub(crate) fn saved_diagram_names(&self, keys: Option<(String, String)>) -> Vec<String> {
        let Some((connection_key, database)) = keys else {
            return Vec::new();
        };
        let mut names: Vec<String> = self
            .diagram_store
            .connections
            .get(&connection_key)
            .and_then(|entry| entry.databases.get(&database))
            .map(|profile| {
                profile
                    .diagrams
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
    pub(crate) fn saved_diagram_summaries(&self) -> Vec<String> {
        self.saved_diagram_entries()
            .into_iter()
            .map(|(_, database, name)| format!("{database} - {name}"))
            .collect()
    }
    pub(crate) fn saved_diagram_entries(&self) -> Vec<(String, String, String)> {
        let connection_label = |key: &str| {
            let parts = key.split('|').collect::<Vec<_>>();
            if parts.first() == Some(&"sqlite") {
                return parts
                    .get(1)
                    .and_then(|path| std::path::Path::new(path).file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or(key)
                    .to_string();
            }
            match parts.as_slice() {
                [driver, host, port, _, username, ..] => {
                    format!("{driver} - {username}@{host}:{port}")
                }
                _ => key.to_string(),
            }
        };
        let mut entries = self
            .diagram_store
            .connections
            .iter()
            .flat_map(|(connection, scope)| {
                let connection = connection_label(connection);
                scope.databases.iter().flat_map(move |(database, profile)| {
                    let connection = connection.clone();
                    profile.diagrams.iter().map(move |diagram| {
                        (connection.clone(), database.clone(), diagram.name.clone())
                    })
                })
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }
    pub(crate) fn clear_saved_diagrams(&mut self) -> Result<usize, String> {
        let store = self.diagram_store.clone();
        let count = self.clear_saved_diagrams_from_store();
        if count == 0 {
            return Ok(0);
        }
        match crate::storage::save_diagram_store(&self.diagram_store) {
            Ok(()) => Ok(count),
            Err(error) => {
                self.diagram_store = store;
                Err(error)
            }
        }
    }
    pub(crate) fn clear_saved_diagrams_from_store(&mut self) -> usize {
        let count = self.saved_diagram_summaries().len();
        self.diagram_store = DiagramStore::default();
        count
    }
    pub(crate) fn remove_saved_diagram_from_store(
        &mut self,
        keys: Option<(String, String)>,
        name: &str,
    ) -> bool {
        let Some((connection_key, database)) = keys else {
            return false;
        };
        if let Some(profile) = self
            .diagram_store
            .connections
            .get_mut(&connection_key)
            .and_then(|entry| entry.databases.get_mut(&database))
        {
            let count = profile.diagrams.len();
            profile.diagrams.retain(|entry| entry.name != name);
            profile.diagrams.len() != count
        } else {
            false
        }
    }
    pub(crate) fn persist_diagram_as(
        &mut self,
        keys: Option<(String, String)>,
        index: usize,
        name: &str,
    ) {
        let Some((connection_key, database)) = keys else {
            return;
        };
        let Some(tab) = self.diagram_tabs.get(index) else {
            return;
        };
        let stored = StoredDiagram::capture(name, &tab.state);
        let profile = self
            .diagram_store
            .connections
            .entry(connection_key)
            .or_default()
            .databases
            .entry(database)
            .or_default();
        match profile
            .diagrams
            .iter_mut()
            .find(|entry| entry.name == stored.name)
        {
            Some(entry) => *entry = stored,
            None => profile.diagrams.push(stored),
        }
        if let Err(error) = crate::storage::save_diagram_store(&self.diagram_store) {
            self.diagram_store_error = Some(error);
        }
    }
    pub(crate) fn restore_diagram_layout(&mut self, keys: Option<(String, String)>, index: usize) {
        let Some((connection_key, database)) = keys else {
            return;
        };
        let Some(title) = self.diagram_tabs.get(index).map(|tab| tab.title.clone()) else {
            return;
        };
        let stored = self
            .diagram_store
            .connections
            .get(&connection_key)
            .and_then(|entry| entry.databases.get(&database))
            .and_then(|profile| profile.diagrams.iter().find(|entry| entry.name == title))
            .cloned();
        if let (Some(stored), Some(tab)) = (stored, self.diagram_tabs.get_mut(index)) {
            stored.restore(&mut tab.state);
        }
    }
}
