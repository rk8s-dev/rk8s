use std::collections::HashMap;

use sea_orm::{
    TryGetError, Value,
    sea_query::{self, ValueTypeErr},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::meta::entities::{PlockMeta, plock_meta};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum FileLockType {
    ReadLock = libc::F_RDLCK as u32,
    WriteLock = libc::F_WRLCK as u32,
    UnLock = libc::F_UNLCK as u32,
}

impl FileLockType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            x if x == Self::ReadLock as u32 => Some(Self::ReadLock),
            x if x == Self::WriteLock as u32 => Some(Self::WriteLock),
            x if x == Self::UnLock as u32 => Some(Self::UnLock),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl std::convert::From<FileLockType> for sea_orm::Value {
    fn from(value: FileLockType) -> Self {
        match value {
            FileLockType::ReadLock => Value::Unsigned(Some(FileLockType::ReadLock as u32)),
            FileLockType::WriteLock => Value::Unsigned(Some(FileLockType::WriteLock as u32)),
            FileLockType::UnLock => Value::Unsigned(Some(FileLockType::UnLock as u32)),
        }
    }
}

impl sea_orm::TryGetable for FileLockType {
    fn try_get_by<I: sea_orm::ColIdx>(
        res: &sea_orm::QueryResult,
        index: I,
    ) -> Result<Self, sea_orm::TryGetError> {
        let val: u32 = res.try_get_by(index)?;
        FileLockType::from_u32(val).ok_or(TryGetError::DbErr(sea_orm::DbErr::Type(
            "Failed to deserialize FIleLockType".to_string(),
        )))
    }
}

impl sea_query::ValueType for FileLockType {
    fn try_from(v: Value) -> Result<Self, sea_query::ValueTypeErr> {
        match v {
            Value::Unsigned(Some(val)) => FileLockType::from_u32(val).ok_or(ValueTypeErr),
            _ => Err(sea_query::ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "FlockType".to_string()
    }

    fn array_type() -> sea_query::ArrayType {
        sea_orm::sea_query::ArrayType::Unsigned
    }

    fn column_type() -> sea_orm::ColumnType {
        sea_orm::sea_query::ColumnType::Unsigned
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlockRecord {
    pub lock_type: FileLockType,
    pub pid: u32,
    pub lock_range: FileLockRange,
}

impl PlockRecord {
    pub fn new(lock_type: FileLockType, pid: u32, start: u64, end: u64) -> Self {
        return Self {
            lock_type,
            pid,
            lock_range: FileLockRange { start, end },
        };
    }

    pub async fn is_conflict(&self, locks: Vec<PlockRecord>) -> bool {
        for lock in locks {
            if self.lock_range.overlaps(&lock.lock_range) {
                match (self.lock_type, lock.lock_type) {
                    (FileLockType::ReadLock, FileLockType::ReadLock) => {}
                    _ => return true,
                }
            }
        }

        false
    }

    pub fn update_locks(mut ls: Vec<PlockRecord>, nl: PlockRecord) -> Vec<PlockRecord> {
        let mut i = 0;
        let mut nl = nl;
        let mut new_records = Vec::new(); // records need to insert

        while i < ls.len() && nl.lock_range.end > nl.lock_range.start {
            let l = ls[i];

            match () {
                _ if l.lock_range.end < nl.lock_range.start => {
                    // skip
                }
                _ if l.lock_range.start < nl.lock_range.start => {
                    // split the current lock
                    let mut left = ls[i];
                    left.lock_range.end = nl.lock_range.start;

                    let middle = PlockRecord::new(
                        nl.lock_type,
                        nl.pid,
                        nl.lock_range.start,
                        l.lock_range.end,
                    );
                    new_records.push((i + 1, middle));

                    ls[i] = left;
                    nl.lock_range.start = l.lock_range.end;
                    i += 1;
                }
                _ if l.lock_range.end < nl.lock_range.end => {
                    // Shrink the current lock range
                    ls[i].lock_type = nl.lock_type;
                    ls[i].lock_range.start = nl.lock_range.start;
                    nl.lock_range.start = l.lock_range.end;
                } // Insert new lock and adjust next lock
                _ if l.lock_range.start < nl.lock_range.end => {
                    new_records.push((i, nl));
                    nl.lock_range.start = nl.lock_range.end;
                }
                _ => {
                    // Insert new lock
                    new_records.push((i, nl));
                    nl.lock_range.start = nl.lock_range.end;
                }
            }

            i += 1;
        }

        // Insert from back to front to avoid index shifting issues
        for (pos, record) in new_records.into_iter().rev() {
            ls.insert(pos, record);
        }
        if nl.lock_range.start < nl.lock_range.end {
            ls.push(PlockRecord::new(
                nl.lock_type,
                nl.pid,
                nl.lock_range.start,
                nl.lock_range.end,
            ));
        }

        // Cleanup and merge
        ls.retain(|r| r.lock_type != FileLockType::UnLock && r.lock_range.start < r.lock_range.end);

        let mut result: Vec<PlockRecord> = Vec::new();
        for record in ls {
            if let Some(last) = result.last_mut() {
                if last.lock_type == record.lock_type
                    && last.lock_range.end == record.lock_range.start
                {
                    last.lock_range.end = record.lock_range.end;
                    continue;
                }
            }
            result.push(record);
        }

        result
    }

    pub fn check_confilct(
        lock_type: &FileLockType,
        range: &FileLockRange,
        ls: &Vec<PlockRecord>,
    ) -> bool {
        for l in ls {
            if (*lock_type == FileLockType::WriteLock || l.lock_type == FileLockType::WriteLock)
                && range.end >= l.lock_range.start
                && range.start <= l.lock_range.end
            {
                return true;
            }
        }

        return false;
    }

    pub fn get_plock(
        locks: &Vec<PlockRecord>,
        query: &FileLockQuery,
        self_sid: &Uuid,
        lock_sid: &Uuid,
    ) -> Option<FileLockInfo> {
        for lock in locks {
            if (lock.lock_type == FileLockType::WriteLock
                || query.lock_type == FileLockType::WriteLock)
                && lock.lock_range.overlaps(&query.range)
            {
                if *self_sid == *lock_sid {
                    return Some(FileLockInfo {
                        lock_type: lock.lock_type,
                        range: lock.lock_range,
                        pid: lock.pid,
                    });
                } else {
                    return Some(FileLockInfo {
                        lock_type: lock.lock_type,
                        range: lock.lock_range,
                        pid: 0,
                    });
                }
            }
        }
        return None;
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct FileLockRange {
    pub start: u64,
    pub end: u64,
}

impl FileLockRange {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.end >= other.start && self.start <= other.end
    }
}
#[derive(Debug, Clone, Copy)]
pub struct FileLockQuery {
    pub owner: u64,
    pub lock_type: FileLockType,
    pub range: FileLockRange,
}

#[derive(Debug, Clone, Copy)]
pub struct FileLockInfo {
    pub lock_type: FileLockType,
    pub range: FileLockRange,
    pub pid: u32,
}

impl FileLockInfo {
    pub fn unlocked() -> Self {
        Self {
            lock_type: FileLockType::UnLock,
            range: FileLockRange::default(),
            pid: 0,
        }
    }
}
