//! Bounded capability/lifetime experiment; see docs/design/0003-capability-model.md.
//!
//! Plan: one scoped, invariant-branded model owns fixed resource, grant and
//! operation tables. Validate before mutation; invalidate tokens by advancing
//! generations, permanently retiring slots at their limit. Recompute references
//! from bounded tables, and retain byte charges until the last grant AND operation
//! disappear. Revocation closes grants; cancellation only requests an outcome.
//!
//! This is metadata, not allocated memory or DMA safety. Owner labels come from
//! a trusted caller, not authentication. `complete` models a trusted backend's
//! quiescence acknowledgement. Dropping this model proves nothing about hardware.

use core::marker::PhantomData;

type Brand<'id> = PhantomData<fn(&'id ()) -> &'id ()>;
pub const RESOURCE_CAPACITY: usize = 64;
pub const GRANT_CAPACITY: usize = 128;
pub const OPERATION_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Owner(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rights(u8);
impl Rights {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1);
    pub const WRITE: Self = Self(2);
    pub const GRANT: Self = Self(4);
    pub const ALL: Self = Self(7);
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidConfig,
    StaleHandle,
    StaleOperation,
    WrongOwner,
    Denied,
    ByteBudget,
    ResourceSlots,
    GrantSlots,
    OperationLimit,
    OperationSlots,
    Overflow,
    Invariant,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub byte_budget: u64,
    pub max_in_flight: usize,
    pub generation_limit: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stats {
    pub resources: usize,
    pub grants: usize,
    pub operations: usize,
    pub bytes: u64,
    pub retired_resources: usize,
    pub retired_grants: usize,
    pub retired_operations: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Handle<'id> {
    slot: usize,
    generation: u32,
    brand: Brand<'id>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Operation<'id> {
    slot: usize,
    generation: u32,
    brand: Brand<'id>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ResourceKey {
    slot: usize,
    generation: u32,
}
#[derive(Clone, Copy)]
struct Resource {
    owner: Owner,
    bytes: u64,
    revoked: bool,
}
#[derive(Clone, Copy)]
struct Grant {
    owner: Owner,
    resource: ResourceKey,
    rights: Rights,
    root: bool,
}
#[derive(Clone, Copy)]
struct Pending {
    owner: Owner,
    resource: ResourceKey,
    cancelled: bool,
}
#[derive(Clone, Copy)]
struct Slot<T: Copy> {
    generation: u32,
    value: Option<T>,
}
impl<T: Copy> Slot<T> {
    const EMPTY: Self = Self {
        generation: 1,
        value: None,
    };
    fn invalidate(&mut self, limit: u32) {
        self.value = None;
        self.generation = if self.generation >= limit {
            0 // Retired forever; no generation wraps back to a valid token.
        } else {
            self.generation.checked_add(1).unwrap_or(0)
        };
    }
}

/// No public constructor or Clone: exactly one model is created per fresh brand.
pub struct Model<'id> {
    config: Config,
    resources: [Slot<Resource>; RESOURCE_CAPACITY],
    grants: [Slot<Grant>; GRANT_CAPACITY],
    operations: [Slot<Pending>; OPERATION_CAPACITY],
    bytes: u64,
    brand: Brand<'id>,
}

/// The higher-ranked callback cannot return branded tokens or mix nested models.
pub fn with_model<R>(config: Config, f: impl for<'id> FnOnce(Model<'id>) -> R) -> Result<R, Error> {
    if config.generation_limit == 0 || config.max_in_flight > OPERATION_CAPACITY {
        return Err(Error::InvalidConfig);
    }
    Ok(f(Model {
        config,
        resources: [Slot::EMPTY; RESOURCE_CAPACITY],
        grants: [Slot::EMPTY; GRANT_CAPACITY],
        operations: [Slot::EMPTY; OPERATION_CAPACITY],
        bytes: 0,
        brand: PhantomData,
    }))
}

fn free<T: Copy>(slots: &[Slot<T>]) -> Option<usize> {
    slots
        .iter()
        .position(|s| s.value.is_none() && s.generation != 0)
}
fn live<T: Copy>(slots: &[Slot<T>]) -> usize {
    slots.iter().filter(|s| s.value.is_some()).count()
}
fn retired<T: Copy>(slots: &[Slot<T>]) -> usize {
    slots.iter().filter(|s| s.generation == 0).count()
}

impl<'id> Model<'id> {
    fn resource(&self, key: ResourceKey) -> Result<Resource, Error> {
        self.resources
            .get(key.slot)
            .filter(|s| s.generation == key.generation)
            .and_then(|s| s.value)
            .ok_or(Error::Invariant)
    }
    fn grant_for(&self, owner: Owner, handle: Handle<'id>) -> Result<Grant, Error> {
        let grant = self
            .grants
            .get(handle.slot)
            .filter(|s| s.generation == handle.generation)
            .and_then(|s| s.value)
            .ok_or(Error::StaleHandle)?;
        if grant.owner != owner {
            return Err(Error::WrongOwner);
        }
        if self.resource(grant.resource)?.revoked {
            return Err(Error::Invariant);
        }
        Ok(grant)
    }
    fn pending(&self, op: Operation<'id>) -> Result<Pending, Error> {
        let pending = self
            .operations
            .get(op.slot)
            .filter(|s| s.generation == op.generation)
            .and_then(|s| s.value)
            .ok_or(Error::StaleOperation)?;
        self.resource(pending.resource)?;
        Ok(pending)
    }
    fn has_grants(&self, key: ResourceKey, except: Option<usize>) -> bool {
        self.grants
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != except && s.value.is_some_and(|g| g.resource == key))
    }
    fn has_operations(&self, key: ResourceKey, except: Option<usize>) -> bool {
        self.operations
            .iter()
            .enumerate()
            .any(|(i, s)| Some(i) != except && s.value.is_some_and(|o| o.resource == key))
    }
    fn reclaimed_bytes(&self, key: ResourceKey, reclaim: bool) -> Result<u64, Error> {
        let resource = self.resource(key)?;
        if reclaim {
            self.bytes
                .checked_sub(resource.bytes)
                .ok_or(Error::Invariant)
        } else {
            Ok(self.bytes)
        }
    }
    fn reclaim(&mut self, key: ResourceKey, reclaim: bool, bytes: u64) {
        if reclaim {
            self.resources[key.slot].invalidate(self.config.generation_limit);
        }
        self.bytes = bytes;
    }

    pub fn create(
        &mut self,
        owner: Owner,
        bytes: u64,
        rights: Rights,
    ) -> Result<Handle<'id>, Error> {
        let total = self.bytes.checked_add(bytes).ok_or(Error::Overflow)?;
        if total > self.config.byte_budget {
            return Err(Error::ByteBudget);
        }
        let resource_slot = free(&self.resources).ok_or(Error::ResourceSlots)?;
        let slot = free(&self.grants).ok_or(Error::GrantSlots)?;
        let resource = ResourceKey {
            slot: resource_slot,
            generation: self.resources[resource_slot].generation,
        };
        self.resources[resource_slot].value = Some(Resource {
            owner,
            bytes,
            revoked: false,
        });
        self.grants[slot].value = Some(Grant {
            owner,
            resource,
            rights,
            root: true,
        });
        self.bytes = total;
        Ok(Handle {
            slot,
            generation: self.grants[slot].generation,
            brand: PhantomData,
        })
    }
    pub fn grant(
        &mut self,
        owner: Owner,
        handle: Handle<'id>,
        to: Owner,
        rights: Rights,
    ) -> Result<Handle<'id>, Error> {
        let parent = self.grant_for(owner, handle)?;
        if !parent.rights.contains(Rights::GRANT) || !parent.rights.contains(rights) {
            return Err(Error::Denied);
        }
        let slot = free(&self.grants).ok_or(Error::GrantSlots)?;
        self.grants[slot].value = Some(Grant {
            owner: to,
            resource: parent.resource,
            rights,
            root: false,
        });
        Ok(Handle {
            slot,
            generation: self.grants[slot].generation,
            brand: PhantomData,
        })
    }
    pub fn begin(
        &mut self,
        owner: Owner,
        handle: Handle<'id>,
        access: Access,
    ) -> Result<Operation<'id>, Error> {
        let grant = self.grant_for(owner, handle)?;
        let right = match access {
            Access::Read => Rights::READ,
            Access::Write => Rights::WRITE,
        };
        if !grant.rights.contains(right) {
            return Err(Error::Denied);
        }
        if live(&self.operations) >= self.config.max_in_flight {
            return Err(Error::OperationLimit);
        }
        let slot = free(&self.operations).ok_or(Error::OperationSlots)?;
        self.operations[slot].value = Some(Pending {
            owner,
            resource: grant.resource,
            cancelled: false,
        });
        Ok(Operation {
            slot,
            generation: self.operations[slot].generation,
            brand: PhantomData,
        })
    }
    /// Only the original root grant can revoke, regardless of ordinary rights.
    /// Revocation does not cancel already admitted operations.
    pub fn revoke(&mut self, owner: Owner, handle: Handle<'id>) -> Result<(), Error> {
        let grant = self.grant_for(owner, handle)?;
        if !grant.root {
            return Err(Error::Denied);
        }
        let reclaim = !self.has_operations(grant.resource, None);
        let bytes = self.reclaimed_bytes(grant.resource, reclaim)?;
        for slot in &mut self.grants {
            if slot.value.is_some_and(|g| g.resource == grant.resource) {
                slot.invalidate(self.config.generation_limit);
            }
        }
        if let Some(resource) = self.resources[grant.resource.slot].value.as_mut() {
            resource.revoked = true;
        }
        self.reclaim(grant.resource, reclaim, bytes);
        Ok(())
    }
    /// Closing the root does not close descendants; they keep their own grants.
    pub fn release(&mut self, owner: Owner, handle: Handle<'id>) -> Result<(), Error> {
        let grant = self.grant_for(owner, handle)?;
        let reclaim = !self.has_grants(grant.resource, Some(handle.slot))
            && !self.has_operations(grant.resource, None);
        let bytes = self.reclaimed_bytes(grant.resource, reclaim)?;
        self.grants[handle.slot].invalidate(self.config.generation_limit);
        self.reclaim(grant.resource, reclaim, bytes);
        Ok(())
    }
    pub fn cancel(&mut self, owner: Owner, op: Operation<'id>) -> Result<(), Error> {
        let pending = self.pending(op)?;
        if pending.owner != owner {
            return Err(Error::WrongOwner);
        }
        self.operations[op.slot].value = Some(Pending {
            cancelled: true,
            ..pending
        });
        Ok(())
    }
    /// Trusted backend acknowledgment, not an operation callable by an owner ABI.
    pub fn complete(&mut self, op: Operation<'id>) -> Result<Completion, Error> {
        let pending = self.pending(op)?;
        let reclaim = !self.has_grants(pending.resource, None)
            && !self.has_operations(pending.resource, Some(op.slot));
        let bytes = self.reclaimed_bytes(pending.resource, reclaim)?;
        self.operations[op.slot].invalidate(self.config.generation_limit);
        self.reclaim(pending.resource, reclaim, bytes);
        Ok(if pending.cancelled {
            Completion::Cancelled
        } else {
            Completion::Completed
        })
    }
    pub fn stats(&self) -> Stats {
        Stats {
            resources: live(&self.resources),
            grants: live(&self.grants),
            operations: live(&self.operations),
            bytes: self.bytes,
            retired_resources: retired(&self.resources),
            retired_grants: retired(&self.grants),
            retired_operations: retired(&self.operations),
        }
    }
    /// Independent bounded scan of references, generations and accounting.
    pub fn audit(&self) -> bool {
        fn valid<T: Copy>(slots: &[Slot<T>], limit: u32) -> bool {
            slots
                .iter()
                .all(|s| s.generation <= limit && (s.generation != 0 || s.value.is_none()))
        }
        if self.config.generation_limit == 0
            || self.config.max_in_flight > OPERATION_CAPACITY
            || !valid(&self.resources, self.config.generation_limit)
            || !valid(&self.grants, self.config.generation_limit)
            || !valid(&self.operations, self.config.generation_limit)
            || live(&self.operations) > self.config.max_in_flight
        {
            return false;
        }
        let mut bytes = 0u64;
        for (i, slot) in self.resources.iter().enumerate() {
            if let Some(resource) = slot.value {
                let key = ResourceKey {
                    slot: i,
                    generation: slot.generation,
                };
                let Some(total) = bytes.checked_add(resource.bytes) else {
                    return false;
                };
                bytes = total;
                let grants = self.has_grants(key, None);
                if (!grants && !self.has_operations(key, None)) || (resource.revoked && grants) {
                    return false;
                }
                if self
                    .grants
                    .iter()
                    .filter(|s| s.value.is_some_and(|g| g.resource == key && g.root))
                    .count()
                    > 1
                {
                    return false;
                }
            }
        }
        for slot in &self.grants {
            if let Some(grant) = slot.value {
                let Ok(resource) = self.resource(grant.resource) else {
                    return false;
                };
                if resource.revoked
                    || (grant.root && grant.owner != resource.owner)
                    || !Rights::ALL.contains(grant.rights)
                {
                    return false;
                }
            }
        }
        for slot in &self.operations {
            if let Some(op) = slot.value
                && self.resource(op.resource).is_err()
            {
                return false;
            }
        }
        bytes == self.bytes && bytes <= self.config.byte_budget
    }
}
