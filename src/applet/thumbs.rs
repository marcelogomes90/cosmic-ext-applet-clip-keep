use std::collections::HashMap;

use cosmic::widget::image;

use crate::clip::model::EntryId;

const CAPACITY: usize = 64;

#[derive(Default)]
pub struct Thumbs {
    loaded: HashMap<EntryId, Option<image::Handle>>,
    order: Vec<EntryId>,
    pending: Vec<EntryId>,
}

impl Thumbs {
    pub fn get(&self, id: EntryId) -> Option<&image::Handle> {
        self.loaded.get(&id).and_then(Option::as_ref)
    }

    pub fn wants(&self, id: EntryId) -> bool {
        !self.loaded.contains_key(&id) && !self.pending.contains(&id)
    }

    pub fn mark_pending(&mut self, id: EntryId) {
        if !self.pending.contains(&id) {
            self.pending.push(id);
        }
    }

    pub fn insert(&mut self, id: EntryId, handle: Option<image::Handle>) {
        self.pending.retain(|pending| *pending != id);
        self.loaded.insert(id, handle);
        self.touch(id);

        while self.order.len() > CAPACITY {
            let evicted = self.order.remove(0);
            self.loaded.remove(&evicted);
        }
    }

    pub fn touch(&mut self, id: EntryId) {
        self.order.retain(|held| *held != id);
        self.order.push(id);
    }

    pub fn clear(&mut self) {
        self.loaded.clear();
        self.order.clear();
        self.pending.clear();
    }

    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> image::Handle {
        image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255])
    }

    #[test]
    fn an_entry_is_wanted_until_it_is_asked_about() {
        let mut thumbs = Thumbs::default();
        let id = EntryId(1);

        assert!(thumbs.wants(id));
        thumbs.mark_pending(id);
        assert!(!thumbs.wants(id), "a load is already in flight");

        thumbs.insert(id, Some(handle()));
        assert!(!thumbs.wants(id));
        assert!(thumbs.get(id).is_some());
    }

    #[test]
    fn an_entry_with_no_thumbnail_is_remembered_as_such() {
        let mut thumbs = Thumbs::default();
        let id = EntryId(7);

        thumbs.insert(id, None);

        assert!(thumbs.get(id).is_none());
        assert!(
            !thumbs.wants(id),
            "asking again on every redraw would be a load per frame"
        );
    }

    #[test]
    fn the_cache_stays_within_its_bound() {
        let mut thumbs = Thumbs::default();

        let capacity = i64::try_from(CAPACITY).unwrap();
        for index in 0..(capacity + 20) {
            thumbs.insert(EntryId(index), Some(handle()));
        }

        assert_eq!(thumbs.len(), CAPACITY);
        assert!(thumbs.get(EntryId(0)).is_none(), "the oldest went first");
        assert!(thumbs.get(EntryId(capacity + 19)).is_some());
    }

    #[test]
    fn drawing_an_entry_again_saves_it_from_eviction() {
        let mut thumbs = Thumbs::default();
        let survivor = EntryId(0);

        thumbs.insert(survivor, Some(handle()));
        for index in 1..i64::try_from(CAPACITY).unwrap() {
            thumbs.insert(EntryId(index), Some(handle()));
        }

        thumbs.touch(survivor);
        thumbs.insert(EntryId(1000), Some(handle()));

        assert!(thumbs.get(survivor).is_some());
        assert!(
            thumbs.get(EntryId(1)).is_none(),
            "the next oldest went instead"
        );
    }

    #[test]
    fn clearing_forgets_everything_including_pending_loads() {
        let mut thumbs = Thumbs::default();
        thumbs.insert(EntryId(1), Some(handle()));
        thumbs.mark_pending(EntryId(2));

        thumbs.clear();

        assert!(thumbs.is_empty());
        assert!(thumbs.wants(EntryId(1)));
        assert!(thumbs.wants(EntryId(2)));
    }
}
