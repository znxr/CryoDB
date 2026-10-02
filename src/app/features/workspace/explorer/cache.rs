use super::State;
use crate::constants::{DEFAULT_TABLE_INFO_CACHE_LIMIT_ENTRIES, TABLE_INFO_CACHE_TTL_SECS};
use crate::model::table::TableInfoDetails;
use std::collections::HashSet;

impl State {
    pub(crate) fn clear_table_metadata_cache(&mut self) {
        self.table_triggers_cache.clear();
        self.table_relations_cache.clear();
        self.sidebar_triggers.clear();
        self.sidebar_relations.clear();
    }
    pub(crate) fn remove_table_metadata_cache(&mut self, table: &str) {
        self.table_triggers_cache.remove(table);
        self.table_relations_cache.remove(table);
        self.sidebar_triggers.retain(|entry| entry.table != table);
        self.sidebar_relations.retain(|entry| entry.table != table);
    }
    pub(crate) fn retain_table_metadata_cache(&mut self, tables: &HashSet<String>) {
        self.table_triggers_cache
            .retain(|table, _| tables.contains(table));
        self.table_relations_cache
            .retain(|table, _| tables.contains(table));
        self.sidebar_triggers
            .retain(|entry| tables.contains(&entry.table));
        self.sidebar_relations
            .retain(|entry| tables.contains(&entry.table));
    }
    fn mark_table_info_cache_recent(&mut self, table: &str) {
        self.table_info_cache_lru.retain(|entry| entry != table);
        self.table_info_cache_lru.push_back(table.to_string());
    }
    pub(crate) fn clear_table_info_cache(&mut self) {
        self.table_info_cache.clear();
        self.table_info_cache_lru.clear();
    }
    pub(crate) fn remove_table_info_cache_entry(&mut self, table: &str) {
        self.table_info_cache.remove(table);
        self.table_info_cache_lru.retain(|entry| entry != table);
    }
    pub(crate) fn retain_table_info_cache(&mut self, tables: &HashSet<String>) {
        self.table_info_cache
            .retain(|table, _| tables.contains(table));
        self.table_info_cache_lru
            .retain(|table| tables.contains(table));
    }
    fn prune_expired_table_info_cache(&mut self) {
        let ttl = std::time::Duration::from_secs(TABLE_INFO_CACHE_TTL_SECS);
        let expired = self
            .table_info_cache
            .iter()
            .filter_map(|(table, (_, cached_at))| {
                if cached_at.elapsed() > ttl {
                    Some(table.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for table in expired {
            self.remove_table_info_cache_entry(&table);
        }
    }
    fn trim_table_info_cache_to_limits(&mut self) {
        while self.table_info_cache.len() > DEFAULT_TABLE_INFO_CACHE_LIMIT_ENTRIES {
            let Some(oldest) = self.table_info_cache_lru.pop_front() else {
                break;
            };
            self.table_info_cache.remove(&oldest);
        }
    }
    pub(crate) fn insert_table_info_cache_entry(&mut self, table: String, info: TableInfoDetails) {
        self.prune_expired_table_info_cache();
        self.table_info_cache
            .insert(table.clone(), (info, std::time::Instant::now()));
        self.mark_table_info_cache_recent(&table);
        self.trim_table_info_cache_to_limits();
    }
    pub(crate) fn get_table_info_cache_entry(&mut self, table: &str) -> Option<TableInfoDetails> {
        let cached = self.table_info_cache.get(table).cloned()?;
        if cached.1.elapsed() > std::time::Duration::from_secs(TABLE_INFO_CACHE_TTL_SECS) {
            self.remove_table_info_cache_entry(table);
            return None;
        }

        self.mark_table_info_cache_recent(table);
        Some(cached.0)
    }
    pub(crate) fn rename_table_info_cache_entry(&mut self, old: &str, new: &str) {
        let Some(entry) = self.table_info_cache.remove(old) else {
            return;
        };
        self.table_info_cache.insert(new.to_string(), entry);
        self.table_info_cache_lru.retain(|value| value != old);
        self.mark_table_info_cache_recent(new);
    }
}
