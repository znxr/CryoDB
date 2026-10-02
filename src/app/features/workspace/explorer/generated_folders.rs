use super::State;
use crate::ai::folders::fallback_generated_folder_for_table;
use crate::model::connection::DatabaseDriver;
use crate::model::table::{GeneratedFolder, TableFolder};
use crate::utils::helpers::canonical_table_key;
use std::collections::{HashMap, HashSet};

impl State {
    pub(crate) fn table_alias_lookup(&self, driver: DatabaseDriver) -> HashMap<String, String> {
        let mut lookup = HashMap::new();
        let mut basename_counts = HashMap::<String, usize>::new();

        if driver == DatabaseDriver::PostgreSql {
            for table in &self.tables {
                let key = canonical_table_key(driver, table);
                if let Some((_, base)) = key.rsplit_once('.') {
                    *basename_counts
                        .entry(base.to_ascii_lowercase())
                        .or_insert(0) += 1;
                }
            }
        }

        for table in &self.tables {
            let trimmed = table.trim();
            if trimmed.is_empty() {
                continue;
            }

            lookup
                .entry(trimmed.to_ascii_lowercase())
                .or_insert_with(|| table.clone());

            let key = canonical_table_key(driver, trimmed);
            if key.is_empty() {
                continue;
            }

            lookup
                .entry(key.to_ascii_lowercase())
                .or_insert_with(|| table.clone());

            if driver == DatabaseDriver::PostgreSql
                && let Some((_, base)) = key.rsplit_once('.')
            {
                let base_key = base.to_ascii_lowercase();
                if basename_counts.get(&base_key).copied().unwrap_or(0) == 1 {
                    lookup.entry(base_key).or_insert_with(|| table.clone());
                }
            }
        }

        lookup
    }
    pub(crate) fn apply_generated_folders(
        &mut self,
        keys: Option<(String, String)>,
        driver: DatabaseDriver,
        groups: Vec<GeneratedFolder>,
    ) -> usize {
        let table_lookup = self.table_alias_lookup(driver);
        let mut folder_lookup = self
            .table_folders
            .iter()
            .enumerate()
            .map(|(index, folder)| (folder.name.to_ascii_lowercase(), index))
            .collect::<HashMap<_, _>>();
        let mut assigned = HashSet::new();

        for group in groups {
            let folder_name = group.folder.trim();
            if folder_name.is_empty() {
                continue;
            }
            let folder = self.ensure_generated_folder(folder_name, &mut folder_lookup);

            for name in group.tables {
                let normalized = name.trim();
                let lookup_key = normalized.to_ascii_lowercase();
                let resolved = table_lookup.get(&lookup_key).cloned().or_else(|| {
                    let canonical = canonical_table_key(driver, normalized);
                    if canonical.is_empty() {
                        None
                    } else {
                        table_lookup.get(&canonical.to_ascii_lowercase()).cloned()
                    }
                });
                let Some(table) = resolved else {
                    continue;
                };

                let table_key = canonical_table_key(driver, &table);
                if table_key.is_empty() {
                    continue;
                }
                self.table_folder_map.insert(table_key, folder.clone());
                assigned.insert(table);
            }
        }

        let all_tables = self.tables.clone();
        for table in all_tables {
            let table_key = canonical_table_key(driver, &table);
            if table_key.is_empty() {
                continue;
            }
            if self.table_folder_map.contains_key(&table_key) {
                continue;
            }
            let fallback = fallback_generated_folder_for_table(&table);
            if fallback.is_empty() {
                continue;
            }
            let folder = self.ensure_generated_folder(&fallback, &mut folder_lookup);
            self.table_folder_map.insert(table_key, folder);
            assigned.insert(table);
        }

        self.sort_folders();
        self.persist_folder_store(keys);
        assigned.len()
    }
    pub(crate) fn export_generated_folders(&self, driver: DatabaseDriver) -> Vec<GeneratedFolder> {
        let table_labels = self
            .tables
            .iter()
            .filter_map(|table| {
                let key = canonical_table_key(driver, table);
                if key.is_empty() {
                    None
                } else {
                    Some((key, table.clone()))
                }
            })
            .collect::<HashMap<_, _>>();

        let mut groups = self
            .table_folders
            .iter()
            .filter(|folder| folder.auto_generated)
            .map(|folder| {
                let mut tables = self
                    .table_folder_map
                    .iter()
                    .filter_map(|(table_key, mapped_folder)| {
                        if mapped_folder.eq_ignore_ascii_case(folder.name.as_str()) {
                            Some(
                                table_labels
                                    .get(table_key)
                                    .cloned()
                                    .unwrap_or_else(|| table_key.clone()),
                            )
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                tables.sort_by(|left, right| {
                    left.to_ascii_lowercase()
                        .cmp(&right.to_ascii_lowercase())
                        .then_with(|| left.cmp(right))
                });

                GeneratedFolder {
                    folder: folder.name.clone(),
                    tables,
                }
            })
            .collect::<Vec<_>>();

        groups.sort_by(|left, right| {
            left.folder
                .to_ascii_lowercase()
                .cmp(&right.folder.to_ascii_lowercase())
                .then_with(|| left.folder.cmp(&right.folder))
        });

        groups
    }
    pub(crate) fn import_generated_folders(
        &mut self,
        keys: Option<(String, String)>,
        driver: DatabaseDriver,
        groups: Vec<GeneratedFolder>,
    ) -> (usize, usize) {
        self.clear_generated_folders(keys.clone());
        if groups.is_empty() {
            return (0, 0);
        }

        let table_lookup = self.table_alias_lookup(driver);
        let mut folder_lookup = self
            .table_folders
            .iter()
            .enumerate()
            .map(|(index, folder)| (folder.name.to_ascii_lowercase(), index))
            .collect::<HashMap<_, _>>();

        let mut imported_folders = HashSet::new();
        let mut assigned = HashSet::new();

        for group in groups {
            let folder_name = group.folder.trim();
            if folder_name.is_empty() {
                continue;
            }

            let folder = self.ensure_generated_folder(folder_name, &mut folder_lookup);
            imported_folders.insert(folder.to_ascii_lowercase());

            for name in group.tables {
                let normalized = name.trim();
                let lookup_key = normalized.to_ascii_lowercase();
                let resolved = table_lookup.get(&lookup_key).cloned().or_else(|| {
                    let canonical = canonical_table_key(driver, normalized);
                    if canonical.is_empty() {
                        None
                    } else {
                        table_lookup.get(&canonical.to_ascii_lowercase()).cloned()
                    }
                });
                let Some(table) = resolved else {
                    continue;
                };

                let table_key = canonical_table_key(driver, &table);
                if table_key.is_empty() {
                    continue;
                }
                self.table_folder_map.insert(table_key, folder.clone());
                assigned.insert(table);
            }
        }

        self.sort_folders();
        self.persist_folder_store(keys);
        (imported_folders.len(), assigned.len())
    }
    pub(crate) fn ensure_generated_folder(
        &mut self,
        folder_name: &str,
        folder_lookup: &mut HashMap<String, usize>,
    ) -> String {
        let trimmed = folder_name.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let folder_key = trimmed.to_ascii_lowercase();
        if let Some(index) = folder_lookup.get(&folder_key).copied()
            && let Some(existing) = self.table_folders.get_mut(index)
        {
            existing.auto_generated = true;
            return existing.name.clone();
        }

        let folder = trimmed.to_string();
        self.table_folders.push(TableFolder {
            name: folder.clone(),
            is_open: true,
            pin_to_top: false,
            auto_generated: true,
        });
        folder_lookup.insert(folder_key, self.table_folders.len().saturating_sub(1));
        folder
    }
    pub(crate) fn clear_generated_folders(&mut self, keys: Option<(String, String)>) -> usize {
        let smart_folders = self
            .table_folders
            .iter()
            .filter(|folder| folder.auto_generated)
            .map(|folder| folder.name.clone())
            .collect::<HashSet<_>>();
        if smart_folders.is_empty() {
            return 0;
        }

        self.table_folders.retain(|folder| !folder.auto_generated);
        let before = self.table_folder_map.len();
        self.table_folder_map
            .retain(|_, folder| !smart_folders.contains(folder));
        self.persist_folder_store(keys);
        before.saturating_sub(self.table_folder_map.len())
    }
    pub(crate) fn set_generated_folders_open(
        &mut self,
        keys: Option<(String, String)>,
        is_open: bool,
    ) -> usize {
        let mut changed = 0;
        for folder in &mut self.table_folders {
            if folder.auto_generated && folder.is_open != is_open {
                folder.is_open = is_open;
                changed += 1;
            }
        }
        if changed > 0 {
            self.persist_folder_store(keys);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::table::FolderStore;

    #[test]
    fn imported_groups_require_unambiguous_postgres_table_names() {
        let mut state = State::new(FolderStore::default(), None);
        state.tables = vec!["sales.users".into(), "audit.users".into()];
        let counts = state.import_generated_folders(
            None,
            DatabaseDriver::PostgreSql,
            vec![GeneratedFolder {
                folder: "People".into(),
                tables: vec!["users".into(), "sales.users".into()],
            }],
        );
        assert_eq!(counts, (1, 1));
        assert_eq!(state.table_folder_map["sales.users"], "People");
        assert!(!state.table_folder_map.contains_key("audit.users"));
        let exported = state.export_generated_folders(DatabaseDriver::PostgreSql);
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].tables, vec!["sales.users"]);
    }
}
