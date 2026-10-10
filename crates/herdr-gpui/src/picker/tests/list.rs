//! The list's rules, which are Herdr's TUI picker's: what a query keeps,
//! when the create row is offered, and what Enter chooses.

use super::*;
use core::prelude::v1::test;

fn list(create: bool, selected: Option<&str>) -> List {
    List::new(
        vec![
            Item::new("work".into(), "Work", Some("3 spaces"), Some("1")),
            Item::new("home".into(), "Home", None, None),
            Item::new("side".into(), "Side project", Some("archived"), None),
        ],
        create.then_some("New group"),
        selected,
    )
}

fn ids(list: &List) -> Vec<&str> {
    list.rows()
        .iter()
        .map(|row| match row {
            Row::Item(index) => list.item(*index).unwrap().id.as_str(),
            Row::Create => "+",
        })
        .collect()
}

#[test]
fn every_item_is_listed_in_order_with_the_named_one_selected() {
    let list = list(true, Some("home"));
    assert_eq!(ids(&list), ["work", "home", "side"]);
    assert_eq!(list.selected(), Some(1));
    assert_eq!(list.choice(), Some(Choice::Item("home".into())));
    // An id no item has, or none at all, selects the first row.
    for selected in [None, Some("gone")] {
        let list = self::list(true, selected);
        assert_eq!(list.selected(), Some(0));
        assert_eq!(list.choice(), Some(Choice::Item("work".into())));
    }
}

#[test]
fn a_query_keeps_items_whose_label_or_detail_contains_it_ignoring_case() {
    let mut list = list(false, Some("home"));
    assert!(list.set_query("wo"));
    assert_eq!(ids(&list), ["work"]);
    // A changed query selects the first row again.
    assert_eq!(list.selected(), Some(0));
    assert!(list.set_query("O"));
    assert_eq!(ids(&list), ["work", "home", "side"]);
    // The detail is searched too, and the query is trimmed.
    assert!(list.set_query("  ARCHIVED "));
    assert_eq!(ids(&list), ["side"]);
    // Label and detail are searched as one text, a space apart.
    assert!(list.set_query("work 3"));
    assert_eq!(ids(&list), ["work"]);
    // The badge and the id are not searched.
    assert!(list.set_query("1"));
    assert_eq!(ids(&list), Vec::<&str>::new());
    assert_eq!(list.choice(), None);
    // The same query again is not a change, and keeps the selection.
    assert!(list.set_query("o"));
    assert!(list.step(1));
    assert!(!list.set_query("o"));
    assert_eq!(list.selected(), Some(1));
    assert!(list.set_query(""));
    assert_eq!(ids(&list), ["work", "home", "side"]);
}

#[test]
fn the_create_row_follows_the_matches_unless_the_query_names_an_item() {
    let mut list = list(true, None);
    // Nothing typed: nothing to create.
    assert_eq!(ids(&list), ["work", "home", "side"]);
    list.set_query("   ");
    assert_eq!(ids(&list), ["work", "home", "side"]);
    // Text that matches items but is not a label still offers to create,
    // after the matches, so Enter keeps choosing the first match.
    list.set_query("wo");
    assert_eq!(ids(&list), ["work", "+"]);
    assert_eq!(list.choice(), Some(Choice::Item("work".into())));
    // A label, whatever its case or surrounding space, withholds the row.
    for query in ["work", " WORK ", "Side Project"] {
        list.set_query(query);
        assert!(!ids(&list).contains(&"+"), "{query}");
    }
    // Text that matches nothing leaves only the create row, with the query
    // as typed apart from its surrounding space.
    list.set_query("  Reading List ");
    assert_eq!(ids(&list), ["+"]);
    assert_eq!(list.create_text(), "Reading List");
    assert_eq!(list.choice(), Some(Choice::Create("Reading List".into())));
    assert_eq!(list.create_label().unwrap().as_ref(), "New group");
}

#[test]
fn without_a_create_row_an_unmatched_query_chooses_nothing() {
    let mut list = list(false, None);
    list.set_query("Reading List");
    assert!(list.rows().is_empty());
    assert_eq!(list.choice(), None);
    assert_eq!(list.create_label(), None);
    assert!(!list.step(1));
    assert!(!list.select(0));
}

#[test]
fn the_selection_moves_a_row_at_a_time_and_stops_at_both_ends() {
    let mut list = list(true, None);
    assert!(!list.step(-1));
    assert!(list.step(1));
    assert!(list.step(1));
    assert_eq!(list.selected(), Some(2));
    assert!(!list.step(1));
    assert_eq!(list.choice(), Some(Choice::Item("side".into())));
    assert!(list.step(-1));
    assert_eq!(list.selected(), Some(1));
    // The create row is a row like any other.
    list.set_query("o");
    assert_eq!(ids(&list), ["work", "home", "side", "+"]);
    assert!(list.select(3));
    assert_eq!(list.choice(), Some(Choice::Create("o".into())));
    assert!(!list.select(4));
    assert_eq!(list.selected(), Some(3));
    // Far steps and overflowing ones stop at the ends too.
    assert!(list.step(isize::MIN));
    assert_eq!(list.selected(), Some(0));
    assert!(list.step(isize::MAX));
    assert_eq!(list.selected(), Some(3));
}

#[test]
fn an_empty_list_has_no_rows_and_no_choice() {
    let mut list = List::new(Vec::new(), None, Some("home"));
    assert_eq!((list.len(), list.rows().len()), (0, 0));
    assert_eq!(list.choice(), None);
    assert!(!list.step(1) && !list.step(-1));
    list.set_query("x");
    assert_eq!(list.choice(), None);
    // With a create row, typing is still a way out.
    let mut list = List::new(Vec::new(), Some("New"), None);
    list.set_query("x");
    assert_eq!(list.choice(), Some(Choice::Create("x".into())));
}

#[test]
fn shown_text_is_cleaned_and_bounded_but_ids_and_matching_are_not() {
    let long = "x".repeat(5000);
    let mut list = List::new(
        vec![
            Item::new(
                "\u{1b}[31mid\n".into(),
                "\u{1b}[31mRed\u{202e} alert\n",
                Some("de\ttail"),
                Some("  \u{7}!  "),
            ),
            Item::new("long".into(), &format!("{long}needle"), None, Some(&long)),
        ],
        Some("New\u{1b} group"),
        None,
    );
    let item = list.item(0).unwrap();
    // Escapes, line breaks and direction overrides are never shown.
    assert_eq!(item.label.as_ref(), "[31mRed alert");
    assert_eq!(item.detail.as_ref(), "detail");
    assert_eq!(item.badge.as_ref(), "!");
    assert_eq!(list.create_label().unwrap().as_ref(), "New group");
    // The id is the plugin's and goes back exactly as it came.
    assert_eq!(list.choice(), Some(Choice::Item("\u{1b}[31mid\n".into())));
    let item = list.item(1).unwrap();
    assert_eq!(item.label.chars().count(), 512);
    assert_eq!(item.badge.chars().count(), 32);
    // Text past what a row shows is still found.
    list.set_query("needle");
    assert_eq!(list.rows(), [Row::Item(1), Row::Create]);
}
