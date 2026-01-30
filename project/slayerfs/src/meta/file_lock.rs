use sea_orm::{
    TryGetError, Value,
    sea_query::{self, ValueTypeErr},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum FileLockType {
    Read = libc::F_RDLCK as u32,
    Write = libc::F_WRLCK as u32,
    UnLock = libc::F_UNLCK as u32,
}

impl FileLockType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            x if x == Self::Read as u32 => Some(Self::Read),
            x if x == Self::Write as u32 => Some(Self::Write),
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
            FileLockType::Read => Value::Unsigned(Some(FileLockType::Read as u32)),
            FileLockType::Write => Value::Unsigned(Some(FileLockType::Write as u32)),
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
            "Failed to deserialize FileLockType".to_string(),
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
        Self {
            lock_type,
            pid,
            lock_range: FileLockRange { start, end },
        }
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
                _ if l.lock_range.end > nl.lock_range.end
                    && l.lock_range.start >= nl.lock_range.start =>
                {
                    // Exact or partial overlap from the right - shrink the current lock
                    ls[i].lock_range.start = nl.lock_range.end;
                    nl.lock_range.start = l.lock_range.end;
                }
                _ if l.lock_range.start < nl.lock_range.start
                    && l.lock_range.end > nl.lock_range.end =>
                {
                    // Unlock range is inside current lock - split into two locks
                    let mut left_part = ls[i];
                    left_part.lock_range.end = nl.lock_range.start;

                    let right_part =
                        PlockRecord::new(l.lock_type, l.pid, nl.lock_range.end, l.lock_range.end);

                    ls[i] = left_part;
                    new_records.push((i + 1, right_part));
                    i += 1;
                }
                _ => {
                    // Exact match or unlock covers the current lock
                    // Remove this lock completely
                    ls.remove(i);
                    nl.lock_range.start = l.lock_range.end;
                    // Don't increment i since we want to process the next element (which shifted to current position)
                    continue; // Skip the i += 1 at the end of this iteration
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
            if let Some(last) = result.last_mut()
                && last.lock_type == record.lock_type
                && last.lock_range.end == record.lock_range.start
            {
                last.lock_range.end = record.lock_range.end;
                continue;
            }
            result.push(record);
        }

        result
    }

    pub fn check_conflict(
        lock_type: &FileLockType,
        range: &FileLockRange,
        ls: &Vec<PlockRecord>,
    ) -> bool {
        for l in ls {
            if (*lock_type == FileLockType::Write || l.lock_type == FileLockType::Write)
                && range.end > l.lock_range.start
                && range.start < l.lock_range.end
            {
                return true;
            }
        }

        false
    }

    pub fn get_plock(
        locks: &Vec<PlockRecord>,
        query: &FileLockQuery,
        self_sid: &Uuid,
        lock_sid: &Uuid,
    ) -> Option<FileLockInfo> {
        for lock in locks {
            if lock.lock_range.overlaps(&query.range) {
                let conflict = !matches!(
                    (lock.lock_type, query.lock_type),
                    (FileLockType::Read, FileLockType::Read)
                );
                if conflict {
                    return Some(FileLockInfo {
                        lock_type: lock.lock_type,
                        range: lock.lock_range,
                        pid: if self_sid == lock_sid { lock.pid } else { 0 },
                    });
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[cfg_attr(
    feature = "rkyv-serialization",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct FileLockRange {
    pub start: u64,
    pub end: u64,
}

impl FileLockRange {
    pub fn overlaps(&self, other: &Self) -> bool {
        self.end > other.start && self.start < other.end
    }
}
#[derive(Debug, Clone, Copy)]
pub struct FileLockQuery {
    pub owner: i64,
    pub lock_type: FileLockType,
    pub range: FileLockRange,
}

#[derive(Debug, Clone, Copy)]
pub struct FileLockInfo {
    pub lock_type: FileLockType,
    pub range: FileLockRange,
    pub pid: u32,
}
