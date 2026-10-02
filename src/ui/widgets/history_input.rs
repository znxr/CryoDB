use iced::widget::{Id, TextInput, text_input};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Factory<M>(Arc<dyn Fn(String) -> M + Send + Sync>);

impl<M> Factory<M> {
    pub(crate) fn new(builder: impl Fn(String) -> M + Send + Sync + 'static) -> Self {
        Self(Arc::new(builder))
    }

    pub(crate) fn message(&self, value: String) -> M {
        (self.0)(value)
    }
}

impl<M> std::fmt::Debug for Factory<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Factory")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Edit<M> {
    pub(crate) id: Id,
    pub(crate) factory: Factory<M>,
    pub(crate) previous: String,
    pub(crate) value: String,
}

impl<M: 'static> Edit<M> {
    pub(crate) fn map<N>(self, map: impl Fn(M) -> N + Send + Sync + 'static) -> Edit<N> {
        Edit {
            id: self.id,
            factory: Factory::new(move |value| map(self.factory.message(value))),
            previous: self.previous,
            value: self.value,
        }
    }
}

pub(crate) fn history_input<'a, M: Clone + 'static>(
    id: Id,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> M + Send + Sync + 'static,
    on_edit: impl Fn(Edit<M>) -> M + 'a,
) -> TextInput<'a, M> {
    let factory = Factory::new(on_input);
    let previous = value.to_string();
    text_input(&crate::i18n::tr(placeholder), value)
        .id(id.clone())
        .on_input(move |value| {
            on_edit(Edit {
                id: id.clone(),
                factory: factory.clone(),
                previous: previous.clone(),
                value,
            })
        })
}
