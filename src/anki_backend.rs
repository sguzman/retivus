//! Feature-gated direct integration with the pinned, AGPL-licensed Anki Rust engine.
//! No user profile is opened by this module's initial construction or tests.
use anki::collection::CollectionBuilder;
use anki::decks::DeckId;
use std::error::Error;

type BackendResult<T> = Result<T, Box<dyn Error>>;

pub struct AnkiBackend {
    collection: anki::collection::Collection,
}

impl AnkiBackend {
    /// Opens a disposable in-memory Anki collection; does not access the user's Anki profile.
    pub fn in_memory() -> BackendResult<Self> {
        Ok(Self {
            collection: CollectionBuilder::default().build()?,
        })
    }

    pub fn create_deck(&mut self, name: &str) -> BackendResult<i64> {
        if name.trim().is_empty() {
            return Err("deck name cannot be blank".into());
        }
        let deck = self.collection.get_or_create_normal_deck(name)?;
        Ok(deck.id.0)
    }

    pub fn add_basic_note(&mut self, deck_id: i64, front: &str, back: &str) -> BackendResult<i64> {
        if front.trim().is_empty() || back.trim().is_empty() {
            return Err("front and back must not be blank".into());
        }
        let notetype = self
            .collection
            .get_notetype_by_name("Basic")?
            .ok_or("Anki collection does not contain Basic note type")?;
        let mut note = notetype.new_note();
        note.set_field(0, front)?;
        note.set_field(1, back)?;
        self.collection.add_note(&mut note, DeckId(deck_id))?;
        Ok(note.id.0)
    }

    pub fn due_counts(&mut self) -> BackendResult<(usize, usize, usize)> {
        let queued = self.collection.get_queued_cards(10, false)?;
        Ok((queued.new_count, queued.learning_count, queued.review_count))
    }

    pub fn next_card_id(&mut self) -> BackendResult<Option<i64>> {
        Ok(self
            .collection
            .get_queued_cards(1, false)?
            .cards
            .first()
            .map(|card| card.card.id().0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anki::search::SortMode;

    #[test]
    fn creates_real_anki_deck_note_and_due_card_without_disk_profile() {
        let mut backend = AnkiBackend::in_memory().unwrap();
        let deck_id = backend.create_deck("Retivus integration").unwrap();
        assert_ne!(deck_id, 1);
        assert!(backend.create_deck(" ").is_err());
        assert!(backend.add_basic_note(1, "", "meaning").is_err());

        let note_id = backend.add_basic_note(1, "banqueta", "sidewalk").unwrap();
        assert!(note_id > 0);
        let cards = backend
            .collection
            .search_cards("front:banqueta", SortMode::NoOrder)
            .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(backend.next_card_id().unwrap(), Some(cards[0].0));
        let (new, learning, review) = backend.due_counts().unwrap();
        assert!(new >= 1);
        assert_eq!((learning, review), (0, 0));
    }
}
