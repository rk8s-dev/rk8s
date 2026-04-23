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
        let mut result = Vec::with_capacity(ls.len() + 1);
        let mut inserted = false;

        for lock in ls.drain(..) {
            if lock.lock_range.end <= nl.lock_range.start {
                result.push(lock);
                continue;
            }

            if lock.lock_range.start >= nl.lock_range.end {
                if !inserted
                    && nl.lock_type != FileLockType::UnLock
                    && nl.lock_range.start < nl.lock_range.end
                {
                    result.push(nl);
                    inserted = true;
                }
                result.push(lock);
                continue;
            }

            if lock.lock_range.start < nl.lock_range.start {
                result.push(PlockRecord::new(
                    lock.lock_type,
                    lock.pid,
                    lock.lock_range.start,
                    nl.lock_range.start,
                ));
            }

            if !inserted
                && nl.lock_type != FileLockType::UnLock
                && nl.lock_range.start < nl.lock_range.end
            {
                result.push(nl);
                inserted = true;
            }

            if lock.lock_range.end > nl.lock_range.end {
                result.push(PlockRecord::new(
                    lock.lock_type,
                    lock.pid,
                    nl.lock_range.end,
                    lock.lock_range.end,
                ));
            }
        }

        if !inserted
            && nl.lock_type != FileLockType::UnLock
            && nl.lock_range.start < nl.lock_range.end
        {
            result.push(nl);
        }

        result.retain(|r| {
            r.lock_type != FileLockType::UnLock && r.lock_range.start < r.lock_range.end
        });

        let mut merged: Vec<PlockRecord> = Vec::with_capacity(result.len());
        for record in result {
            if let Some(last) = merged.last_mut()
                && last.lock_type == record.lock_type
                && last.pid == record.pid
                && last.lock_range.end == record.lock_range.start
            {
                last.lock_range.end = record.lock_range.end;
                continue;
            }
            merged.push(record);
        }

        merged
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

#[cfg(test)]
mod tests {
    use super::{FileLockType, PlockRecord};

    #[test]
    fn update_locks_keeps_adjacent_ranges_separate() {
        let locks = vec![PlockRecord::new(FileLockType::Write, 1, 10, 20)];
        let updated = PlockRecord::update_locks(
            locks,
            PlockRecord::new(FileLockType::Write, 1, 20, 30),
        );

        assert_eq!(
            updated,
            vec![PlockRecord::new(FileLockType::Write, 1, 10, 30)]
        );
    }

    #[test]
    fn update_locks_replaces_only_the_overlapping_region() {
        let locks = vec![PlockRecord::new(FileLockType::Write, 1, 10, 40)];
        let updated = PlockRecord::update_locks(
            locks,
            PlockRecord::new(FileLockType::Read, 1, 20, 30),
        );

        assert_eq!(
            updated,
            vec![
                PlockRecord::new(FileLockType::Write, 1, 10, 20),
                PlockRecord::new(FileLockType::Read, 1, 20, 30),
                PlockRecord::new(FileLockType::Write, 1, 30, 40),
            ]
        );
    }

    #[test]
    fn update_locks_unlock_splits_existing_range() {
        let locks = vec![PlockRecord::new(FileLockType::Write, 1, 10, 40)];
        let updated = PlockRecord::update_locks(
            locks,
            PlockRecord::new(FileLockType::UnLock, 1, 20, 30),
        );

        assert_eq!(
            updated,
            vec![
                PlockRecord::new(FileLockType::Write, 1, 10, 20),
                PlockRecord::new(FileLockType::Write, 1, 30, 40),
            ]
        );
    }

    #[test]
    fn update_locks_only_merges_same_pid() {
        let locks = vec![PlockRecord::new(FileLockType::Write, 1, 10, 20)];
        let updated = PlockRecord::update_locks(
            locks,
            PlockRecord::new(FileLockType::Write, 2, 20, 30),
        );

        assert_eq!(
            updated,
            vec![
                PlockRecord::new(FileLockType::Write, 1, 10, 20),
                PlockRecord::new(FileLockType::Write, 2, 20, 30),
            ]
        );
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
