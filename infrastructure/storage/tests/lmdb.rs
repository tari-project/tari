// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::TryFrom,
    fs::File,
    io::{BufRead, BufReader},
    net::Ipv4Addr,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    thread,
};

use serde::{Deserialize, Serialize};
use tari_storage::{
    lmdb_store::{db, LMDBBuilder, LMDBConfig, LMDBDatabase, LMDBError, LMDBStore},
    IterationResult,
};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct User {
    id: u64,
    first: String,
    last: String,
    email: String,
    male: bool,
    ip: Ipv4Addr,
}

impl User {
    fn new(csv: &str) -> Result<User, String> {
        let vals: Vec<&str> = csv.split(',').collect();
        if vals.len() != 6 {
            return Err("Incomplete Record".into());
        }
        let id = u64::from_str(vals[0]).map_err(|e| e.to_string())?;
        let first = vals[1].to_string();
        let last = vals[2].to_string();
        let email = vals[3].to_string();
        let male = vals[4] == "Male";
        let ip = Ipv4Addr::from_str(vals[5]).map_err(|e| e.to_string())?;
        Ok(User {
            id,
            first,
            last,
            email,
            male,
            ip,
        })
    }
}

fn get_path(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(name);
    path.to_str().unwrap().to_string()
}

fn init(name: &str) -> Result<LMDBStore, LMDBError> {
    let path = get_path(name);
    std::fs::create_dir_all(&path).unwrap_or_default();
    LMDBBuilder::new()
        .set_path(&path)
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(2)
        .add_database("users", db::CREATE)
        .build()
}

fn clean_up(name: &str) {
    std::fs::remove_dir_all(get_path(name)).unwrap();
}

fn load_users() -> Vec<User> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/users.csv");
    let file = File::open(path).unwrap();
    BufReader::new(file)
        .lines() // `BufReader::lines` is platform agnostic, recognises both `\r\n` and `\n`
        .map(|result| result.unwrap())
        .map(|s| User::new(&s).unwrap())
        .collect()
}

fn insert_all_users(name: &str) -> (Vec<User>, LMDBDatabase) {
    let users = load_users();
    let env = init(name).unwrap();
    let db = env.get_handle("users").unwrap();
    let res = db.with_write_transaction(|mut db| {
        for user in &users {
            db.insert(&user.id, &user)?;
        }
        Ok(())
    });
    assert!(res.is_ok());
    (users, db)
}

#[test]
fn test_single_thread() {
    {
        let users = load_users();
        let env = init("single_thread").unwrap();
        let db = env.get_handle("users").unwrap();
        for user in &users {
            db.insert(&user.id, &user).unwrap();
        }
        for user in &users {
            let check: User = db.get(&user.id).unwrap().unwrap();
            assert_eq!(check, *user);
        }
        assert_eq!(db.len().unwrap(), 1000);
    }
    clean_up("single_thread"); // In Windows file handles must be released before files can be deleted
}

#[test]
fn test_multi_thread() {
    {
        let users_arc = Arc::new(load_users());
        let env = init("multi_thread").unwrap();
        let mut threads = Vec::new();
        for i in 0..10 {
            let db = env.get_handle("users").unwrap();
            let users = users_arc.clone();
            threads.push(thread::spawn(move || {
                for j in 0..100 {
                    let user = &users[i * 100 + j];
                    db.insert(&user.id, user).unwrap();
                }
            }));
        }

        for thread in threads {
            thread.join().unwrap();
        }

        env.log_info();
        let db = env.get_handle("users").unwrap();
        for user in users_arc.iter() {
            let check: User = db.get(&user.id).unwrap().unwrap();
            assert_eq!(check, *user);
        }
    }
    clean_up("multi_thread"); // In Windows file handles must be released before files can be deleted
}

#[test]
fn test_transactions() {
    {
        let (users, db) = insert_all_users("transactions");
        // Test the `exists` and value retrieval functions
        db.with_read_transaction(|txn| {
            for user in &users {
                assert!(txn.exists(&user.id).unwrap());
                let check: User = txn.get(&user.id).unwrap().unwrap();
                assert_eq!(check, *user);
            }
        })
        .unwrap();
    }
    clean_up("transactions"); // In Windows file handles must be released before files can be deleted
}

/// Simultaneous writes in different threads
#[test]
#[allow(clippy::same_item_push)]
fn test_multi_thread_writes() {
    {
        let env = init("multi-thread-writes").unwrap();
        let mut threads = Vec::new();
        for _ in 0..2 {
            let db = env.get_handle("users").unwrap();
            threads.push(thread::spawn(move || {
                let res = db.with_write_transaction(|mut txn| {
                    for j in 0..1000 {
                        txn.insert(&j, &j)?;
                    }
                    Ok(())
                });
                assert!(res.is_ok());
            }));
        }
        for thread in threads {
            thread.join().unwrap()
        }
        env.log_info();

        let db = env.get_handle("users").unwrap();

        assert_eq!(db.len().unwrap(), 1000);
        for i in 0..1000 {
            let value: i32 = db.get(&i).unwrap().unwrap();
            assert_eq!(i, value);
        }
    }
    clean_up("multi-thread-writes"); // In Windows file handles must be released before files can be deleted
}

/// Multiple write transactions in a single thread
#[test]
fn test_multi_writes() {
    {
        let store = init("multi-writes").unwrap();
        for i in 0..2 {
            let db = store.get_handle("users").unwrap();
            let res = db.with_write_transaction(|mut txn| {
                for j in 0..1000 {
                    let v = i * 1000 + j;
                    txn.insert(&v, &v)?;
                }
                db.log_info();
                Ok(())
            });
            assert!(res.is_ok());
        }
        store.flush().unwrap();
    }
    clean_up("multi-writes"); // In Windows file handles must be released before files can be deleted
}

#[test]
fn test_pair_iterator() {
    {
        let (users, db) = insert_all_users("pair_iterator");
        let res = db.for_each::<u64, User, _>(|pair| {
            let (key, user) = pair.unwrap();
            assert_eq!(user.id, key);
            assert_eq!(users[usize::try_from(key).unwrap() - 1], user);
            IterationResult::Continue
        });
        assert!(res.is_ok());
    }
    clean_up("pair_iterator"); // In Windows file handles must be released before files can be deleted
}

#[test]
fn test_exists_and_delete() {
    {
        let (_, db) = insert_all_users("delete");
        assert!(db.contains_key(&525u64).unwrap());
        db.remove(&525u64).unwrap();
        assert!(!db.contains_key(&525u64).unwrap());
    }
    clean_up("delete"); // In Windows file handles must be released before files can be deleted
}

#[test]
fn test_lmdb_resize_on_create() {
    let db_env_name = "resize";
    {
        let path = get_path(db_env_name);
        std::fs::create_dir_all(&path).unwrap_or_default();
        let size_used_round_1: usize;
        const PRESET_SIZE: usize = 1;
        let db_name = "test";
        {
            // Create db with large preset environment size
            let store = LMDBBuilder::new()
                .set_path(&path)
                .set_env_config(LMDBConfig::new(
                    100 * PRESET_SIZE * 1024 * 1024,
                    1024 * 1024,
                    512 * 1024,
                ))
                .set_max_number_of_databases(1)
                .add_database(db_name, db::CREATE)
                .build()
                .unwrap();
            // Add some data that is `>= 2 * (PRESET_SIZE * 1024 * 1024)`
            let db = store.get_handle(db_name).unwrap();
            let users = load_users();
            for i in 0..100 {
                db.insert(&i, &users).unwrap();
            }
            // Ensure enough data is loaded
            let env_info = store.env().info().unwrap();
            let env_stat = store.env().stat().unwrap();
            size_used_round_1 = env_stat.psize as usize * env_info.last_pgno;
            assert!(size_used_round_1 >= 2 * (PRESET_SIZE * 1024 * 1024));
            store.flush().unwrap();
        }

        {
            // Load existing db environment
            let env = LMDBBuilder::new()
                .set_path(&path)
                .set_env_config(LMDBConfig::new(PRESET_SIZE * 1024 * 1024, 1024 * 1024, 512 * 1024))
                .set_max_number_of_databases(1)
                .add_database(db_name, db::CREATE)
                .build()
                .unwrap();
            // Ensure `mapsize` is automatically adjusted
            let env_info = env.env().info().unwrap();
            let env_stat = env.env().stat().unwrap();
            let size_used_round_2 = env_stat.psize as usize * env_info.last_pgno;
            let space_remaining = env_info.mapsize - size_used_round_2;
            assert_eq!(size_used_round_1, size_used_round_2);
            assert!(space_remaining > PRESET_SIZE * 1024 * 1024);
            assert!(env_info.mapsize >= 2 * (PRESET_SIZE * 1024 * 1024));
        }
    }
    clean_up(db_env_name); // In Windows file handles must be released before files can be deleted
}

#[test]
fn test_lmdb_resize_before_full() {
    let db_env_name = "resize_dynamic";
    {
        let path = get_path(db_env_name);
        std::fs::create_dir_all(&path).unwrap_or_default();
        let db_name = "test_full";
        {
            // Create db with 1MB capacity
            let store = LMDBBuilder::new()
                .set_path(&path)
                .set_env_config(LMDBConfig::new(1024 * 1024, 512 * 1024, 100 * 1024))
                .set_max_number_of_databases(1)
                .add_database(db_name, db::CREATE)
                .build()
                .unwrap();
            let db = store.get_handle(db_name).unwrap();

            // Add enough data to exceed our 1MB db
            let value = load_users();
            // one insertion requires approx 92KB so after ~11 insertions
            // our 1MB env size should be out of space
            // however the db should now be allocating additional space as it fills up
            for key in 0..32 {
                db.insert(&key, &value).unwrap();
            }
            let env_info = store.env().info().unwrap();
            let psize = store.env().stat().unwrap().psize as usize;
            let page_size_total = psize * env_info.last_pgno;
            let percent_left = 1.0 - page_size_total as f64 / env_info.mapsize as f64;

            // check the allocated size is now greater than it was initially
            assert!(page_size_total > 1024 * 1024);
            assert!(percent_left > 0.0);

            store.flush().unwrap();
        }
    }
    clean_up(db_env_name); // In Windows file handles must be released before files can be deleted
}
