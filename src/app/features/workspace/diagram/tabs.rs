use super::model::{DiagramState, DiagramTab};
use super::{Context, Message, Output, State};
use crate::app::types::ToastLevel;
use iced::Task;
impl State {
    pub(crate) fn open_schema_diagram(
        &mut self,
        context: &Context,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        outputs.push(Output::HideTools);
        if let Some(index) = (!self.diagram_tabs.is_empty()).then_some(0) {
            let index = self
                .active_diagram_tab
                .filter(|index| *index < self.diagram_tabs.len())
                .unwrap_or(index);
            self.active_diagram_tab = Some(index);
            return {
                outputs.push(Output::TabActivated(index));
                Task::none()
            };
        }
        self.new_schema_diagram(context, outputs)
    }
    pub(crate) fn new_schema_diagram(
        &mut self,
        context: &Context,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        outputs.push(Output::HideTools);
        let title = format!("New Diagram #{}", self.next_diagram_tab_number);
        self.next_diagram_tab_number = self.next_diagram_tab_number.saturating_add(1);
        self.diagram_tabs.push(DiagramTab {
            id: self.next_diagram_tab_id,
            title,
            pinned: false,
            state: DiagramState::default(),
        });
        self.next_diagram_tab_id = self.next_diagram_tab_id.saturating_add(1);
        let index = self.diagram_tabs.len() - 1;
        outputs.push(Output::TabAdded(index));
        self.active_diagram_tab = Some(index);
        Task::batch(vec![self.load_schema_diagram(context), {
            outputs.push(Output::TabActivated(index));
            Task::none()
        }])
    }
    pub(crate) fn activate_diagram_tab(
        &mut self,
        outputs: &mut Vec<Output>,
        index: usize,
    ) -> Task<Message> {
        if index >= self.diagram_tabs.len() {
            return Task::none();
        }
        self.active_diagram_tab = Some(index);
        {
            outputs.push(Output::TabActivated(index));
            Task::none()
        }
    }
    pub(crate) fn close_schema_diagram(
        &mut self,
        outputs: &mut Vec<Output>,
        index: usize,
    ) -> Task<Message> {
        if index >= self.diagram_tabs.len() {
            return Task::none();
        }
        self.remove_schema_diagram(outputs, index);
        {
            outputs.push(Output::EnsureTab);
            Task::none()
        }
    }
    pub(crate) fn remove_schema_diagram(&mut self, outputs: &mut Vec<Output>, index: usize) {
        self.diagram_tabs.remove(index);
        outputs.push(Output::TabRemoved(index));
        self.active_diagram_tab = match self.active_diagram_tab {
            Some(active) if active == index => None,
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
    }
    pub(crate) fn open_saved_diagram(
        &mut self,
        context: &Context,
        outputs: &mut Vec<Output>,
        name: String,
    ) -> Task<Message> {
        if let Some(index) = self.diagram_tabs.iter().position(|tab| tab.title == name) {
            return self.activate_diagram_tab(outputs, index);
        }
        self.diagram_tabs.push(DiagramTab {
            id: self.next_diagram_tab_id,
            title: name,
            pinned: false,
            state: DiagramState::default(),
        });
        self.next_diagram_tab_id = self.next_diagram_tab_id.saturating_add(1);
        let index = self.diagram_tabs.len() - 1;
        outputs.push(Output::TabAdded(index));
        self.active_diagram_tab = Some(index);
        Task::batch(vec![self.load_schema_diagram(context), {
            outputs.push(Output::TabActivated(index));
            Task::none()
        }])
    }
    pub(crate) fn delete_saved_diagram(
        &mut self,
        context: &Context,
        outputs: &mut Vec<Output>,
        name: &str,
    ) -> Result<bool, String> {
        if !self.remove_saved_diagram_from_store(context.keys.clone(), name) {
            return Ok(false);
        }
        crate::storage::save_diagram_store(&self.diagram_store)?;
        for index in (0..self.diagram_tabs.len()).rev() {
            if self.diagram_tabs[index].title == name {
                self.remove_schema_diagram(outputs, index);
            }
        }
        outputs.push(Output::EnsureTab);
        Ok(true)
    }
    pub(crate) fn diagram_open_selected_table(
        &mut self,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        let Some(name) = self.active_diagram().and_then(|state| {
            state
                .selected
                .and_then(|index| state.diagram.tables.get(index))
                .map(|table| table.name.clone())
        }) else {
            return Task::none();
        };
        {
            outputs.push(Output::OpenTable(name));
            Task::none()
        }
    }
    pub(crate) fn diagram_remove_change(
        &mut self,
        context: &Context,
        outputs: &mut Vec<Output>,
        index: usize,
    ) -> Task<Message> {
        let Some(removed) = self
            .active_diagram_mut()
            .map(|state| state.remove_change(index))
        else {
            return Task::none();
        };
        if removed {
            self.load_schema_diagram(context)
        } else {
            outputs.push(Output::Toast(
                ToastLevel::Info,
                ("Remove later dependent changes before removing this one.").into(),
            ));
            Task::none()
        }
    }
}
