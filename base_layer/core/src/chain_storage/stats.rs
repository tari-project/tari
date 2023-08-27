//  Copyright 2021, The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt::{Display, Formatter},
    iter::FromIterator,
};

use lmdb_zero as lmdb;

#[derive(Debug, Clone)]
pub struct DbBasicStats {
    root: DbStat,
    env_info: EnvInfo,
    db_stats: Vec<DbStat>,
}

impl DbBasicStats {
    pub(super) fn new<I: IntoIterator<Item = (&'static str, lmdb::Stat)>>(
        global: lmdb::Stat,
        env_info: lmdb::EnvInfo,
        db_stats: I,
    ) -> Self {
        Self {
            root: ("[root]", global).into(),
            env_info: env_info.into(),
            db_stats: db_stats.into_iter().map(Into::into).collect(),
        }
    }

    pub fn root(&self) -> &DbStat {
        &self.root
    }

    pub fn env_info(&self) -> &EnvInfo {
        &self.env_info
    }

    pub fn db_stats(&self) -> &[DbStat] {
        &self.db_stats
    }
}

impl Display for DbBasicStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Root: psize = {}, {}", self.root.psize, self.root)?;
        for stat in &self.db_stats {
            writeln!(f, "{}", stat)?;
        }
        Ok(())
    }
}

/// Statistics information about an environment.
#[derive(Debug, Clone, Copy)]
pub struct DbStat {
    /// Name of the db
    pub name: &'static str,
    /// Size of a database page. This is currently the same for all databases.
    pub psize: u32,
    /// Depth (height) of the B-tree
    pub depth: u32,
    /// Number of internal (non-leaf) pages
    pub branch_pages: usize,
    /// Number of leaf pages
    pub leaf_pages: usize,
    /// Number of overflow pages
    pub overflow_pages: usize,
    /// Number of data items
    pub entries: usize,
}

impl DbStat {
    /// Returns the total size in bytes of all pages
    pub fn total_page_size(&self) -> usize {
        self.psize as usize * (self.leaf_pages + self.branch_pages + self.overflow_pages)
    }
}

impl From<(&'static str, lmdb::Stat)> for DbStat {
    fn from((name, stat): (&'static str, lmdb::Stat)) -> Self {
        Self {
            name,
            psize: stat.psize,
            depth: stat.depth,
            branch_pages: stat.branch_pages,
            leaf_pages: stat.leaf_pages,
            overflow_pages: stat.overflow_pages,
            entries: stat.entries,
        }
    }
}

impl Display for DbStat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "name: {}, Total page size: {}, entries: {}, depth: {}, branch_pages: {}, leaf_pages: {}, overflow_pages: \
             {}",
            self.name,
            self.total_page_size(),
            self.entries,
            self.depth,
            self.branch_pages,
            self.leaf_pages,
            self.overflow_pages,
        )
    }
}

#[derive(Debug, Clone)]
pub struct DbTotalSizeStats {
    sizes: Vec<DbSize>,
}
impl DbTotalSizeStats {
    pub fn sizes(&self) -> &[DbSize] {
        &self.sizes
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DbSize {
    pub name: &'static str,
    pub num_entries: u64,
    pub total_key_size: u64,
    pub total_value_size: u64,
}

impl DbSize {
    pub fn total(&self) -> u64 {
        self.total_key_size.saturating_add(self.total_value_size)
    }

    pub fn avg_bytes_per_entry(&self) -> u64 {
        if self.num_entries == 0 {
            return 0;
        }

        self.total() / self.num_entries
    }
}

impl From<Vec<DbSize>> for DbTotalSizeStats {
    fn from(sizes: Vec<DbSize>) -> Self {
        Self { sizes }
    }
}

impl FromIterator<DbSize> for DbTotalSizeStats {
    fn from_iter<T: IntoIterator<Item = DbSize>>(iter: T) -> Self {
        Self {
            sizes: iter.into_iter().collect(),
        }
    }
}

/// Configuration information about an environment.
#[derive(Debug, Clone, Copy)]
pub struct EnvInfo {
    /// Size of the data memory map
    pub mapsize: usize,
    /// ID of the last used page
    pub last_pgno: usize,
    /// ID of the last committed transaction
    pub last_txnid: usize,
    /// max reader slots in the environment
    pub maxreaders: u32,
    /// max reader slots used in the environment
    pub numreaders: u32,
}

impl From<lmdb::EnvInfo> for EnvInfo {
    fn from(info: lmdb::EnvInfo) -> Self {
        Self {
            mapsize: info.mapsize,
            last_pgno: info.last_pgno,
            last_txnid: info.last_txnid,
            maxreaders: info.maxreaders,
            numreaders: info.numreaders,
        }
    }
}

impl Display for EnvInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mapsize: {:.2} MiB, last_pgno: {}, last_txnid: {}, maxreaders: {}, numreaders: {}",
            self.mapsize as f32 / 1024.0 / 1024.0,
            self.last_pgno,
            self.last_txnid,
            self.maxreaders,
            self.numreaders,
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl DbStat {
        pub fn sample() -> Self {
            DbStat {
                name: "coverage",
                psize: 10,
                depth: 0,
                leaf_pages: 1,
                branch_pages: 2,
                overflow_pages: 3,
                entries: 0,
            }
        }
    }

    impl DbSize {
        pub fn sample() -> Self {
            Self {
                name: "coverage",
                num_entries: 0,
                total_key_size: u64::MAX,
                total_value_size: 1,
            }
        }
    }

    impl EnvInfo {
        pub fn sample() -> Self {
            Self {
                mapsize: 0,
                last_pgno: 0,
                last_txnid: 0,
                maxreaders: 0,
                numreaders: 0,
            }
        }
    }

    impl DbBasicStats {
        pub fn sample() -> Self {
            Self {
                root: DbStat::sample(),
                env_info: EnvInfo::sample(),
                db_stats: vec![DbStat::sample()],
            }
        }
    }

    #[test]
    fn coverage_db_stat() {
        let obj = DbStat::sample();
        assert_eq!(obj.total_page_size(), 60);
    }

    #[test]
    fn coverage_db_basic_stats() {
        let obj = DbBasicStats::sample();
        obj.root();
        obj.env_info();
        obj.db_stats();
    }

    #[test]
    fn coverage_db_size() {
        let mut obj = DbSize::sample();
        assert_eq!(obj.total(), u64::MAX);
        assert_eq!(obj.avg_bytes_per_entry(), 0);
        obj.num_entries = obj.total();
        assert_eq!(obj.avg_bytes_per_entry(), 1);
    }

    #[test]
    fn coverage_db_total_size_stats() {
        let vec = vec![DbSize::sample()];
        let obj = DbTotalSizeStats::from(vec);
        let obj = obj.sizes.into_iter().collect::<DbTotalSizeStats>();
        obj.sizes();
    }
}
