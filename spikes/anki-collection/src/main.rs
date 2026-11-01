//! Isolated upstream compatibility probe. Never uses a real user collection.
use anki::collection::CollectionBuilder;
use anki::decks::DeckId;
use anki::search::SortMode;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // CollectionBuilder::default creates an in-memory collection, not a user profile.
    let mut collection = CollectionBuilder::default().build()?;
    let scratch_deck = collection.get_or_create_normal_deck("Retivus integration smoke")?;
    assert_ne!(scratch_deck.id, DeckId(1));

    let basic = collection
        .get_notetype_by_name("Basic")?
        .expect("A new collection should provide the Basic note type");
    let mut note = basic.new_note();
    note.set_field(0, "banqueta")?;
    note.set_field(1, "sidewalk")?;
    collection.add_note(&mut note, DeckId(1))?;

    let matches = collection.search_cards("front:banqueta", SortMode::NoOrder)?;
    assert_eq!(matches.len(), 1, "Expected one created card");
    let due = collection.get_queued_cards(10, false)?;
    assert!(due.new_count > 0, "Created card should be due as new");
    assert!(
        due.cards.iter().any(|card| card.card.id == matches[0]),
        "Created card must be reachable in the review queue"
    );
    println!("Verified disposable Anki deck, note, card, and review queue");
    Ok(())
}
