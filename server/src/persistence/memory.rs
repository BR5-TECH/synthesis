//! The in-memory adapter of the repository ports.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`.
//! Requirements: SAS-FR-IRWO, SAS-FR-PDNU, SAS-FR-HEIV, SAS-FR-XUFA.
//!
//! One map behind one lock, per kind of record. The store holds nothing between
//! processes: a restart loses every record, which is the whole of the V1
//! durability contract.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use crate::application::ports::{Ports, Repository};
use crate::domain::clock::{Clock, SystemClock};
use crate::domain::error::DomainError;
use crate::domain::ids::{IdSource, RandomIds};

/// The records of one kind, held in memory.
pub struct MemoryRepository<K, V> {
    /// The name the refusals of this store report.
    resource: &'static str,
    /// The insertion order, so a list is stable between calls.
    order: Mutex<Vec<K>>,
    records: Mutex<HashMap<K, V>>,
}

impl<K, V> MemoryRepository<K, V>
where
    K: Copy + Eq + Hash + Send + Sync,
    V: Clone + Send + Sync,
{
    /// A store that holds no record.
    pub fn new(resource: &'static str) -> Self {
        MemoryRepository {
            resource,
            order: Mutex::new(Vec::new()),
            records: Mutex::new(HashMap::new()),
        }
    }
}

impl<K, V> Repository<K, V> for MemoryRepository<K, V>
where
    K: Copy + Eq + Hash + Send + Sync,
    V: Clone + Send + Sync,
{
    fn insert(&self, id: K, value: V) -> Result<V, DomainError> {
        let mut records = self.records.lock().expect("the store is not poisoned");
        if records.contains_key(&id) {
            return Err(DomainError::DuplicateId {
                resource: self.resource,
            });
        }
        records.insert(id, value.clone());
        self.order
            .lock()
            .expect("the order is not poisoned")
            .push(id);
        Ok(value)
    }

    fn find(&self, id: K) -> Option<V> {
        self.records
            .lock()
            .expect("the store is not poisoned")
            .get(&id)
            .cloned()
    }

    fn get(&self, id: K) -> Result<V, DomainError> {
        self.find(id).ok_or(DomainError::NotFound {
            resource: self.resource,
        })
    }

    fn update(
        &self,
        id: K,
        change: &mut dyn FnMut(&mut V) -> Result<(), DomainError>,
    ) -> Result<V, DomainError> {
        let mut records = self.records.lock().expect("the store is not poisoned");
        let record = records.get_mut(&id).ok_or(DomainError::NotFound {
            resource: self.resource,
        })?;

        // The change is applied to a copy. A closure that refuses leaves the
        // stored record untouched (SAS-FR-XUFA).
        let mut candidate = record.clone();
        change(&mut candidate)?;
        *record = candidate.clone();
        Ok(candidate)
    }

    fn remove(&self, id: K) -> Result<V, DomainError> {
        let removed = self
            .records
            .lock()
            .expect("the store is not poisoned")
            .remove(&id)
            .ok_or(DomainError::NotFound {
                resource: self.resource,
            })?;
        self.order
            .lock()
            .expect("the order is not poisoned")
            .retain(|held| *held != id);
        Ok(removed)
    }

    fn list(&self) -> Vec<V> {
        let records = self.records.lock().expect("the store is not poisoned");
        self.order
            .lock()
            .expect("the order is not poisoned")
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect()
    }
}

/// Every port, backed by memory, with the clock and the identifier source the
/// running service uses.
pub fn memory_ports() -> Ports {
    Ports {
        users: Arc::new(MemoryRepository::new("user")),
        teams: Arc::new(MemoryRepository::new("team")),
        organizations: Arc::new(MemoryRepository::new("organization")),
        memberships: Arc::new(MemoryRepository::new("membership")),
        invitations: Arc::new(MemoryRepository::new("invitation")),
        devices: Arc::new(MemoryRepository::new("device")),
        projects: Arc::new(MemoryRepository::new("project")),
        grants: Arc::new(MemoryRepository::new("grant")),
        drafts: Arc::new(MemoryRepository::new("draft")),
        conversations: Arc::new(MemoryRepository::new("conversation")),
        clock: Arc::new(SystemClock) as Arc<dyn Clock>,
        ids: Arc::new(RandomIds) as Arc<dyn IdSource>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MemoryRepository<u32, String> {
        MemoryRepository::new("record")
    }

    // SAS-FR-OZET: a second record with one identifier is refused.
    #[test]
    fn a_duplicate_identifier_is_refused_and_changes_nothing() {
        let store = store();
        store
            .insert(1, "first".to_string())
            .expect("the first write");
        let error = store
            .insert(1, "second".to_string())
            .expect_err("the second write is refused");
        assert_eq!(error.code(), "duplicate_id");
        assert_eq!(store.get(1).expect("the record stands"), "first");
    }

    // SAS-FR-XUFA: a refused change leaves the record as it was.
    #[test]
    fn a_refused_change_leaves_the_record_as_it_was() {
        let store = store();
        store.insert(1, "first".to_string()).expect("the write");
        let error = store
            .update(1, &mut |record: &mut String| {
                record.push_str(" changed");
                Err(DomainError::OwnerProtected)
            })
            .expect_err("the change is refused");
        assert_eq!(error.code(), "owner_protected");
        assert_eq!(store.get(1).expect("the record stands"), "first");
    }

    #[test]
    fn a_list_holds_the_records_in_insertion_order() {
        let store = store();
        for (id, value) in [(3, "c"), (1, "a"), (2, "b")] {
            store.insert(id, value.to_string()).expect("the write");
        }
        assert_eq!(store.list(), vec!["c", "a", "b"]);
        store.remove(1).expect("the removal");
        assert_eq!(store.list(), vec!["c", "b"]);
        assert_eq!(
            store.remove(1).expect_err("a second removal").code(),
            "not_found"
        );
    }

    // SAS-FR-HEIV: concurrent updates are serialized, so a counter never repeats.
    #[test]
    fn concurrent_updates_are_serialized() {
        let store: Arc<MemoryRepository<u32, u64>> = Arc::new(MemoryRepository::new("counter"));
        store.insert(1, 0).expect("the write");

        let threads: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        store
                            .update(1, &mut |value: &mut u64| {
                                *value += 1;
                                Ok(())
                            })
                            .expect("the change");
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().expect("the thread completes");
        }

        assert_eq!(store.get(1).expect("the record"), 800);
    }

    // SAS-FR-PDNU: a new store holds nothing, which is what a restart leaves.
    #[test]
    fn a_new_store_holds_nothing() {
        assert!(memory_ports().users.list().is_empty());
        assert!(memory_ports().projects.list().is_empty());
    }
}
