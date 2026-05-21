#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkKind {
    Handler,
    Function,
    Screen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkRef {
    pub app: u16,
    pub kind: ChunkKind,
    pub index: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChunkCacheSlot {
    key: ChunkRef,
    preload: bool,
    active: bool,
    last_used: u32,
    occupied: bool,
}

impl ChunkCacheSlot {
    const fn empty() -> Self {
        Self {
            key: ChunkRef {
                app: 0,
                kind: ChunkKind::Handler,
                index: 0,
            },
            preload: false,
            active: false,
            last_used: 0,
            occupied: false,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ChunkCacheError {
    Full,
    Missing,
}

pub struct ChunkCache<const N: usize> {
    slots: [ChunkCacheSlot; N],
    clock: u32,
}

impl<const N: usize> ChunkCache<N> {
    pub const fn new() -> Self {
        Self {
            slots: [ChunkCacheSlot::empty(); N],
            clock: 0,
        }
    }

    pub fn insert(&mut self, key: ChunkRef, preload: bool) -> Result<(), ChunkCacheError> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.occupied && slot.key == key)
        {
            slot.preload = preload;
            slot.last_used = self.clock;
            return Ok(());
        }
        if let Some(slot) = self.slots.iter_mut().find(|slot| !slot.occupied) {
            *slot = ChunkCacheSlot {
                key,
                preload,
                active: false,
                last_used: self.clock,
                occupied: true,
            };
            return Ok(());
        }
        let Some(index) = self.evict_candidate_index() else {
            return Err(ChunkCacheError::Full);
        };
        self.slots[index] = ChunkCacheSlot {
            key,
            preload,
            active: false,
            last_used: self.clock,
            occupied: true,
        };
        Ok(())
    }

    pub fn begin_execute(&mut self, key: ChunkRef) -> Result<(), ChunkCacheError> {
        self.clock = self.clock.wrapping_add(1);
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.occupied && slot.key == key)
        else {
            return Err(ChunkCacheError::Missing);
        };
        slot.active = true;
        slot.last_used = self.clock;
        Ok(())
    }

    pub fn end_execute(&mut self, key: ChunkRef) -> Result<(), ChunkCacheError> {
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.occupied && slot.key == key)
        else {
            return Err(ChunkCacheError::Missing);
        };
        slot.active = false;
        Ok(())
    }

    pub fn contains(&self, key: ChunkRef) -> bool {
        self.slots
            .iter()
            .any(|slot| slot.occupied && slot.key == key)
    }

    pub fn drop_app(&mut self, app: u16) {
        for slot in &mut self.slots {
            if slot.occupied && slot.key.app == app {
                *slot = ChunkCacheSlot::empty();
            }
        }
    }

    fn evict_candidate_index(&self) -> Option<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.occupied && !slot.active)
            .min_by_key(|(_, slot)| (slot.preload, slot.last_used))
            .map(|(index, _)| index)
    }
}
