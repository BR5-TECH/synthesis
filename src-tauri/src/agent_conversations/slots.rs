//! The call slots of AGC-FR-25: the bound on the turns that hold a model call,
//! and the slot that a turn lends away while it waits for its advisors
//! (AGC-FR-HVQD).

use super::*;

impl TurnRegistry {
    /// How many of the AGC-FR-25 slots are free right now.
    ///
    /// A hint rather than a reservation — it is read to explain a wait in the
    /// log, never to decide one, which is [`acquire`](Self::acquire)'s business
    /// under the condvar.
    pub(super) fn permits_available(&self) -> usize {
        let (lock, _) = &*self.permits;
        *lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The AGC-FR-25 bound this registry was built with, for the record that
    /// reports a turn waiting on it.
    pub(super) fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Take one of the AGC-FR-25 slots, blocking until one frees. The turn is
    /// already registered and already reported by the time this is reached, so
    /// waiting here delays a call and never refuses a dispatch.
    pub(super) fn acquire(&self) -> PermitGuard {
        let (lock, cvar) = &*self.permits;
        let mut available = lock.lock().unwrap_or_else(|e| e.into_inner());
        while *available == 0 {
            available = cvar.wait(available).unwrap_or_else(|e| e.into_inner());
        }
        *available -= 1;
        PermitGuard { permits: self.permits.clone() }
    }

    /// AGC-FR-25: take the slot of a turn and record it under the turn id. The
    /// slot frees when the returned value drops, which is when the turn ends.
    pub(super) fn take_slot(&self, turn_id: &str) -> TurnSlot<'_> {
        let guard = self.acquire();
        self.held_slots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(turn_id.to_string(), Some(guard));
        TurnSlot { turns: self, turn_id: turn_id.to_string() }
    }

    /// AGC-FR-HVQD: free the slot of a turn that waits for its advisors. The turn
    /// takes a slot again before its next model call ([`ensure_slot`]). A turn
    /// that holds no slot is not changed.
    ///
    /// [`ensure_slot`]: Self::ensure_slot
    pub(super) fn release_slot(&self, turn_id: &str) {
        let released = {
            let mut slots = self.held_slots.lock().unwrap_or_else(|e| e.into_inner());
            slots.get_mut(turn_id).and_then(Option::take)
        };
        drop(released);
    }

    /// AGC-FR-HVQD: take a slot again for a turn that freed its own. Returns
    /// whether the turn had to take one, which can block as `acquire` does.
    pub(super) fn ensure_slot(&self, turn_id: &str) -> bool {
        let lent = {
            let slots = self.held_slots.lock().unwrap_or_else(|e| e.into_inner());
            matches!(slots.get(turn_id), Some(None))
        };
        if !lent {
            return false;
        }
        let guard = self.acquire();
        let mut slots = self.held_slots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = slots.get_mut(turn_id) {
            *entry = Some(guard);
        }
        true
    }
}

/// AGC-FR-25: the record of the slot of one turn. Dropping it frees the slot
/// that the turn holds at that moment.
pub(super) struct TurnSlot<'a> {
    turns: &'a TurnRegistry,
    turn_id: String,
}

impl Drop for TurnSlot<'_> {
    fn drop(&mut self) {
        let removed = {
            let mut slots = self.turns.held_slots.lock().unwrap_or_else(|e| e.into_inner());
            slots.remove(&self.turn_id)
        };
        drop(removed);
    }
}

pub(super) struct PermitGuard {
    pub(super) permits: Arc<(Mutex<usize>, Condvar)>,
}

impl Drop for PermitGuard {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.permits;
        if let Ok(mut available) = lock.lock() {
            *available += 1;
            cvar.notify_one();
        }
    }
}
