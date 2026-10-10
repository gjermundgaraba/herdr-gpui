//! What a picker lists and which row is chosen, with no window or daemon in
//! it: items, the query that filters them, the selection, the optional
//! "create" row, and the choice Enter makes.
//!
//! The rules are Herdr's TUI picker's (`ClientPickOverlay::rows` in its
//! `src/client/shell/state.rs`), so a plugin's picker answers the same way in
//! either client: a query keeps the items whose label or detail contains it,
//! ignoring case, in the order they were given; the create row follows them
//! when the query names no item.

use crate::notifications::safe_text;
use gpui::SharedString;

/// Characters of a label or detail a row shows. The text is a peer's; what a
/// row cannot show is not shaped either. The whole text is still searched.
const SHOWN_CHARS: usize = 512;
/// A badge is a short marker; a longer one is cut rather than crowding the label.
const BADGE_CHARS: usize = 32;

/// One row a picker offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    /// What choosing the row answers with; never shown and never altered.
    pub(crate) id: String,
    pub(crate) label: SharedString,
    /// Secondary text at the row's trailing edge; empty when there is none.
    pub(crate) detail: SharedString,
    /// Short marker before the label; empty when there is none.
    pub(crate) badge: SharedString,
    /// The label and detail as given, lowercased once: what a query searches.
    haystack: String,
    /// The label, trimmed and lowercased: what a query must equal to name
    /// this item, which is what withholds the create row.
    name: String,
}

impl Item {
    pub(crate) fn new(id: String, label: &str, detail: Option<&str>, badge: Option<&str>) -> Self {
        Self {
            id,
            label: safe_text(label, SHOWN_CHARS).into(),
            detail: safe_text(detail.unwrap_or_default(), SHOWN_CHARS).into(),
            badge: safe_text(badge.unwrap_or_default(), BADGE_CHARS)
                .trim()
                .to_owned()
                .into(),
            haystack: format!("{label} {}", detail.unwrap_or_default()).to_lowercase(),
            name: label.trim().to_lowercase(),
        }
    }
}

/// A visible row: an item by its index, or the create row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Item(usize),
    Create,
}

/// What choosing a row means to whoever opened the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    /// An item, by its id.
    Item(String),
    /// The create row, with the query as typed, trimmed.
    Create(String),
}

#[derive(Debug)]
pub(crate) struct List {
    items: Vec<Item>,
    /// The create row's label prefix, such as "New group". `None` offers no
    /// create row.
    create: Option<SharedString>,
    query: String,
    /// The rows `query` leaves, recomputed only when it changes.
    rows: Vec<Row>,
    /// Index into `rows`.
    selected: usize,
}

impl List {
    /// `selected` is the id of the item selected at first; an id no item has
    /// selects the first row.
    pub(crate) fn new(items: Vec<Item>, create: Option<&str>, selected: Option<&str>) -> Self {
        let mut list = Self {
            items,
            create: create.map(|label| safe_text(label, SHOWN_CHARS).into()),
            query: String::new(),
            rows: Vec::new(),
            selected: 0,
        };
        list.rows = list.matching();
        list.selected = selected
            .and_then(|id| {
                list.rows
                    .iter()
                    .position(|row| matches!(row, Row::Item(index) if list.items[*index].id == id))
            })
            .unwrap_or(0);
        list
    }

    fn matching(&self) -> Vec<Row> {
        let query = self.query.trim().to_lowercase();
        let mut rows: Vec<Row> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| query.is_empty() || item.haystack.contains(&query))
            .map(|(index, _)| Row::Item(index))
            .collect();
        if self.create.is_some()
            && !query.is_empty()
            && !self.items.iter().any(|item| item.name == query)
        {
            rows.push(Row::Create);
        }
        rows
    }

    /// Filters by `query`. A changed query selects the first row again.
    /// Returns whether it changed.
    pub(crate) fn set_query(&mut self, query: &str) -> bool {
        if self.query == query {
            return false;
        }
        self.query = query.to_owned();
        self.rows = self.matching();
        self.selected = 0;
        true
    }

    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn item(&self, index: usize) -> Option<&Item> {
        self.items.get(index)
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// Moves the selection by `delta` rows, stopping at either end.
    /// Returns whether it moved.
    pub(crate) fn step(&mut self, delta: isize) -> bool {
        let last = self.rows.len().saturating_sub(1);
        let next = self.selected.saturating_add_signed(delta).min(last);
        let moved = next != self.selected;
        self.selected = next;
        moved
    }

    /// Selects `row` when it exists, as pointing at it does.
    pub(crate) fn select(&mut self, row: usize) -> bool {
        if row >= self.rows.len() {
            return false;
        }
        self.selected = row;
        true
    }

    /// The create row's label prefix.
    pub(crate) fn create_label(&self) -> Option<&SharedString> {
        self.create.as_ref()
    }

    /// The text the create row would create: the query, trimmed.
    pub(crate) fn create_text(&self) -> &str {
        self.query.trim()
    }

    /// What Enter chooses: the selected row, or nothing when no row is left.
    pub(crate) fn choice(&self) -> Option<Choice> {
        Some(match *self.rows.get(self.selected)? {
            Row::Item(index) => Choice::Item(self.items.get(index)?.id.clone()),
            Row::Create => Choice::Create(self.create_text().to_owned()),
        })
    }
}
