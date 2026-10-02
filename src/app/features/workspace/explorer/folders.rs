use super::State;
use crate::model::connection::DatabaseDriver;
use crate::model::settings::PickerOption;
use crate::model::table::{FolderProfileState, StoredFolderEntry, TableFolder};
use crate::storage::save_folder_store;
use crate::utils::helpers::canonical_table_key;
use std::collections::HashSet;

impl State {
    pub(crate) fn folder_exists(&self, folder: &str) -> bool {
        let folder = folder.trim();
        if folder.is_empty() {
            return false;
        }
        self.table_folders
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(folder))
    }
    pub(crate) fn sort_folders(&mut self) {
        self.table_folders.sort_by(|left, right| {
            right.pin_to_top.cmp(&left.pin_to_top).then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
        });
    }
    pub(crate) fn folder_picker_options(&self) -> Vec<PickerOption<String>> {
        let mut options = vec![PickerOption::new(String::new(), "No folder")];
        options.extend(
            self.table_folders
                .iter()
                .map(|folder| PickerOption::new(folder.name.clone(), folder.name.clone())),
        );
        options
    }
    pub(crate) fn folder_for_table(&self, driver: DatabaseDriver, table: &str) -> Option<String> {
        let key = canonical_table_key(driver, table);
        if key.is_empty() {
            return None;
        }
        self.table_folder_map.get(&key).cloned()
    }
    pub(crate) fn table_has_folder_assignment(&self, driver: DatabaseDriver, table: &str) -> bool {
        let key = canonical_table_key(driver, table);
        if key.is_empty() {
            return false;
        }
        self.table_folder_map.contains_key(&key)
    }
    fn normalize_table_folder_map_keys(&mut self, driver: DatabaseDriver) {
        let previous = std::mem::take(&mut self.table_folder_map);
        self.table_folder_map = previous
            .into_iter()
            .filter_map(|(table, folder)| {
                let key = canonical_table_key(driver, &table);
                let folder = folder.trim();
                if key.is_empty() || folder.is_empty() {
                    None
                } else {
                    Some((key, folder.to_string()))
                }
            })
            .collect();
    }
    pub(crate) fn persist_folder_store(&mut self, keys: Option<(String, String)>) {
        let Some((connection_key, database)) = keys else {
            return;
        };

        let connection_entry = self
            .folder_store
            .connections
            .entry(connection_key.clone())
            .or_default();
        if self.table_folders.is_empty() && self.table_folder_map.is_empty() {
            connection_entry.databases.remove(&database);
        } else {
            let folders = self
                .table_folders
                .iter()
                .map(|folder| StoredFolderEntry {
                    name: folder.name.clone(),
                    is_open: folder.is_open,
                    pin_to_top: folder.pin_to_top,
                    auto_generated: folder.auto_generated,
                })
                .collect::<Vec<_>>();
            connection_entry.databases.insert(
                database,
                FolderProfileState {
                    folders,
                    table_folder_map: self.table_folder_map.clone(),
                },
            );
        }
        if connection_entry.databases.is_empty() {
            self.folder_store.connections.remove(&connection_key);
        }

        match save_folder_store(&self.folder_store) {
            Ok(()) => self.folder_store_error = None,
            Err(error) => self.folder_store_error = Some(error),
        }
    }
    pub(crate) fn load_folder_state_for_current_database(
        &mut self,
        driver: DatabaseDriver,
        keys: Option<(String, String)>,
    ) {
        let Some((connection_key, database)) = keys.as_ref() else {
            self.table_folders.clear();
            self.table_folder_map.clear();
            return;
        };

        let Some(profile) = self
            .folder_store
            .connections
            .get(connection_key)
            .and_then(|entry| entry.databases.get(database))
        else {
            self.table_folders.clear();
            self.table_folder_map.clear();
            return;
        };

        self.table_folders = profile
            .folders
            .iter()
            .filter_map(|folder| {
                let name = folder.name.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(TableFolder {
                        name: name.to_string(),
                        is_open: folder.is_open,
                        pin_to_top: folder.pin_to_top,
                        auto_generated: folder.auto_generated,
                    })
                }
            })
            .collect();
        self.table_folder_map = profile.table_folder_map.clone();
        self.normalize_table_folder_map_keys(driver);
        self.reconcile_table_folder_state(driver, keys);
    }
    pub(crate) fn reconcile_table_folder_state(
        &mut self,
        driver: DatabaseDriver,
        keys: Option<(String, String)>,
    ) {
        let previous_folders = self.table_folders.clone();
        let previous_map = self.table_folder_map.clone();

        self.normalize_table_folder_map_keys(driver);

        let mut seen = HashSet::new();
        self.table_folders.retain(|folder| {
            let name = folder.name.trim();
            if name.is_empty() {
                return false;
            }
            seen.insert(name.to_ascii_lowercase())
        });

        let table_names = self
            .tables
            .iter()
            .map(|table| canonical_table_key(driver, table))
            .filter(|table| !table.is_empty())
            .collect::<HashSet<_>>();
        let folder_names = self
            .table_folders
            .iter()
            .map(|folder| folder.name.clone())
            .collect::<HashSet<_>>();
        self.table_folder_map
            .retain(|table, folder| table_names.contains(table) && folder_names.contains(folder));
        self.sort_folders();

        if self.table_folders != previous_folders || self.table_folder_map != previous_map {
            self.persist_folder_store(keys);
        }
    }
    pub(crate) fn assign_table_to_folder(
        &mut self,
        driver: DatabaseDriver,
        keys: Option<(String, String)>,
        table: &str,
        folder: Option<String>,
    ) {
        if !self.tables.iter().any(|name| name == table) {
            return;
        }
        let table_key = canonical_table_key(driver, table);
        if table_key.is_empty() {
            return;
        }
        match folder {
            Some(folder) => {
                let folder = folder.trim().to_string();
                if folder.is_empty() || !self.folder_exists(&folder) {
                    self.table_folder_map.remove(&table_key);
                } else {
                    self.table_folder_map.insert(table_key, folder);
                }
            }
            None => {
                self.table_folder_map.remove(&table_key);
            }
        }
        self.persist_folder_store(keys);
    }
    pub(crate) fn open_folder_for_table(
        &mut self,
        driver: DatabaseDriver,
        keys: Option<(String, String)>,
        table: &str,
    ) {
        let Some(folder) = self.folder_for_table(driver, table) else {
            return;
        };
        if let Some(entry) = self
            .table_folders
            .iter_mut()
            .find(|entry| entry.name == folder)
            && !entry.is_open
        {
            entry.is_open = true;
            self.persist_folder_store(keys);
        }
    }
    pub(crate) fn prune_folder_store_databases(
        &mut self,
        connection_key: Option<String>,
        databases: &[String],
    ) {
        let Some(connection_key) = connection_key else {
            return;
        };
        let Some(connection) = self.folder_store.connections.get_mut(&connection_key) else {
            return;
        };
        connection
            .databases
            .retain(|database, _| databases.iter().any(|db| db == database));
        if connection.databases.is_empty() {
            self.folder_store.connections.remove(&connection_key);
        }
        match save_folder_store(&self.folder_store) {
            Ok(()) => self.folder_store_error = None,
            Err(error) => self.folder_store_error = Some(error),
        }
    }
}

impl State {
    pub(crate) fn dissolve_folder(
        &mut self,
        folder_name: &str,
        keys: Option<(String, String)>,
    ) -> bool {
        if !self.folder_exists(folder_name) {
            return false;
        }
        self.table_folders
            .retain(|folder| folder.name != folder_name);
        self.table_folder_map
            .retain(|_, folder| folder != folder_name);
        self.persist_folder_store(keys);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_databases_restores_saved_folders_without_reusing_the_previous_profile() {
        let store = serde_json::from_str(r#"{
            "connections": {"host|3306|user": {"databases": {"first": {
                "folders": [{"name":"Accounts", "is_open":true, "pin_to_top":true, "smart_group":true}],
                "table_folder_map": {"users":"Accounts"}
            }}}}
        }"#).unwrap();
        let mut state = State::new(store, None);
        state.tables = vec!["users".into()];
        let keys = |database: &str| Some(("host|3306|user".into(), database.into()));
        state.load_folder_state_for_current_database(DatabaseDriver::MySql, keys("first"));
        assert_eq!(
            state
                .folder_for_table(DatabaseDriver::MySql, "users")
                .as_deref(),
            Some("Accounts")
        );
        assert!(state.table_folders[0].auto_generated);
        state.load_folder_state_for_current_database(DatabaseDriver::MySql, keys("second"));
        assert!(state.table_folders.is_empty());
        assert!(state.table_folder_map.is_empty());
        state.load_folder_state_for_current_database(DatabaseDriver::MySql, keys("first"));
        assert!(state.table_folders[0].is_open);
        assert!(state.table_folders[0].pin_to_top);
        assert_eq!(state.table_folder_map["users"], "Accounts");
    }
}
