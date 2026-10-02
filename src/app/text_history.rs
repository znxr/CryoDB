use crate::Message;
use iced::widget::Id;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TEXT_HISTORY_LIMIT: usize = 200;
const TEXT_HISTORY_COALESCE: Duration = Duration::from_millis(450);
const TEXT_HISTORY_FIELDS: usize = 128;

pub(crate) type TextEditFactory = crate::ui::widgets::history_input::Factory<Message>;

#[derive(Debug, Default)]
struct TextFieldHistory {
    undo: Vec<String>,
    redo: Vec<String>,
    current: String,
    factory: Option<TextEditFactory>,
    last_edit: Option<Instant>,
    touched: u64,
}

#[derive(Debug, Default)]
pub(crate) struct TextHistory {
    fields: HashMap<Id, TextFieldHistory>,
    clock: u64,
}

impl TextHistory {
    pub(crate) fn record(
        &mut self,
        id: &Id,
        factory: &TextEditFactory,
        previous: String,
        value: &str,
    ) {
        if !self.fields.contains_key(id) && self.fields.len() >= TEXT_HISTORY_FIELDS {
            self.evict_least_recent();
        }

        self.clock += 1;
        let clock = self.clock;
        let entry = self.fields.entry(id.clone()).or_default();
        entry.factory = Some(factory.clone());
        entry.touched = clock;

        if entry.current == value {
            return;
        }

        let now = Instant::now();
        let within_burst = entry
            .last_edit
            .is_some_and(|at| now.duration_since(at) < TEXT_HISTORY_COALESCE);

        if !within_burst || entry.undo.is_empty() {
            entry.undo.push(previous);
            if entry.undo.len() > TEXT_HISTORY_LIMIT {
                entry.undo.remove(0);
            }
        }

        entry.redo.clear();
        entry.current = value.to_string();
        entry.last_edit = Some(now);
    }

    fn evict_least_recent(&mut self) {
        let Some(oldest) = self
            .fields
            .iter()
            .min_by_key(|(_, entry)| entry.touched)
            .map(|(id, _)| id.clone())
        else {
            return;
        };
        self.fields.remove(&oldest);
    }

    pub(crate) fn undo(&mut self, id: &Id) -> Option<Message> {
        let entry = self.fields.get_mut(id)?;
        let factory = entry.factory.clone()?;
        let previous = entry.undo.pop()?;
        let restored = std::mem::replace(&mut entry.current, previous.clone());
        entry.redo.push(restored);
        entry.last_edit = None;
        Some(factory.message(previous))
    }

    pub(crate) fn redo(&mut self, id: &Id) -> Option<Message> {
        let entry = self.fields.get_mut(id)?;
        let factory = entry.factory.clone()?;
        let next = entry.redo.pop()?;
        let undone = std::mem::replace(&mut entry.current, next.clone());
        entry.undo.push(undone);
        entry.last_edit = None;
        Some(factory.message(next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory() -> TextEditFactory {
        TextEditFactory::new(|value| {
            Message::Connections(crate::app::features::connections::Message::HostChanged(
                value,
            ))
        })
    }

    fn value_of(message: &Message) -> String {
        match message {
            Message::Connections(crate::app::features::connections::Message::HostChanged(
                value,
            )) => value.clone(),
            other => panic!("unexpected message: {other:?}"),
        }
    }

    fn edit(history: &mut TextHistory, id: &Id, previous: &str, value: &str) {
        history.record(id, &factory(), previous.to_string(), value);
        history.fields.get_mut(id).unwrap().last_edit = None;
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let id = Id::from("field");
        let mut history = TextHistory::default();

        edit(&mut history, &id, "", "a");
        edit(&mut history, &id, "a", "ab");

        assert_eq!(value_of(&history.undo(&id).unwrap()), "a");
        assert_eq!(value_of(&history.undo(&id).unwrap()), "");
        assert!(history.undo(&id).is_none());

        assert_eq!(value_of(&history.redo(&id).unwrap()), "a");
        assert_eq!(value_of(&history.redo(&id).unwrap()), "ab");
        assert!(history.redo(&id).is_none());
    }

    #[test]
    fn a_new_edit_drops_the_redo_branch() {
        let id = Id::from("field");
        let mut history = TextHistory::default();

        edit(&mut history, &id, "", "a");
        assert_eq!(value_of(&history.undo(&id).unwrap()), "");

        edit(&mut history, &id, "", "z");
        assert!(history.redo(&id).is_none());
    }

    #[test]
    fn edits_inside_the_burst_window_collapse_into_one_step() {
        let id = Id::from("field");
        let mut history = TextHistory::default();

        history.record(&id, &factory(), String::new(), "a");
        history.record(&id, &factory(), "a".to_string(), "ab");
        history.record(&id, &factory(), "ab".to_string(), "abc");

        assert_eq!(value_of(&history.undo(&id).unwrap()), "");
        assert!(history.undo(&id).is_none());
    }

    #[test]
    fn tracking_many_fields_drops_the_least_recently_edited() {
        let mut history = TextHistory::default();
        let first = Id::from("field-0");
        for index in 0..(TEXT_HISTORY_FIELDS + 1) {
            edit(&mut history, &Id::from(format!("field-{index}")), "", "x");
        }

        assert_eq!(history.fields.len(), TEXT_HISTORY_FIELDS);
        assert!(history.undo(&first).is_none(), "oldest field was evicted");
        let newest = Id::from(format!("field-{TEXT_HISTORY_FIELDS}"));
        assert!(history.undo(&newest).is_some(), "newest field survives");
    }

    #[test]
    fn histories_are_independent_per_field() {
        let left = Id::from("left");
        let right = Id::from("right");
        let mut history = TextHistory::default();

        edit(&mut history, &left, "", "l");
        edit(&mut history, &right, "", "r");

        assert_eq!(value_of(&history.undo(&left).unwrap()), "");
        assert_eq!(value_of(&history.undo(&right).unwrap()), "");
    }
}
