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

use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, path::PathBuf, str::FromStr, sync::Arc, thread};
use tari_storage::lmdb_store::{db, LMDBBuilder, LMDBDatabase, LMDBError, LMDBStore};
use tari_utilities::ExtendBytes;

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
        let vals: Vec<&str> = csv.split(",").collect();
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

impl ExtendBytes for User {
    fn append_raw_bytes(&self, buf: &mut Vec<u8>) {
        self.id.append_raw_bytes(buf);
        self.first.append_raw_bytes(buf);
        self.last.append_raw_bytes(buf);
        self.email.append_raw_bytes(buf);
        self.male.append_raw_bytes(buf);
        buf.extend_from_slice(&self.ip.to_string().as_bytes());
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
    let _ = std::fs::create_dir(&path).unwrap_or_default();
    LMDBBuilder::new()
        .set_path(&path)
        .set_environment_size(10)
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
    let f = std::fs::read_to_string(path).unwrap();
    f.split("\n").map(|s| User::new(s).unwrap()).collect()
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
fn single_thread() {
    let users = load_users();
    let env = init("single_thread").unwrap();
    let db = env.get_handle("users").unwrap();
    for user in &users {
        db.insert(&user.id, &user).unwrap();
    }
    for user in users.iter() {
        let check: User = db.get(&user.id).unwrap().unwrap();
        assert_eq!(check, *user);
    }
    assert_eq!(db.len().unwrap(), 1000);
    clean_up("single_thread");
}

#[test]
fn multi_thread() {
    let _ = simple_logger::init();
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
        ;
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
    clean_up("multi_thread");
}

#[test]
fn transactions() {
    let (users, db) = insert_all_users("transactions");
    // Test the `exists` and value retrieval functions
    let res = db.with_read_transaction::<_, User>(|txn| {
        for user in users.iter() {
            assert!(txn.exists(&user.id).unwrap());
            let check: User = txn.get(&user.id).unwrap().unwrap();
            assert_eq!(check, *user);
        }
        Ok(None)
    });
    assert!(res.unwrap().is_none());
    clean_up("transactions");
}

/// Simultaneous writes in different threads
#[test]
fn multi_thread_writes() {
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
        ;
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

    clean_up("multi-thread-writes");
}

/// Multiple write transactions in a single thread
#[test]
fn multi_writes() {
    let env = init("multi-writes").unwrap();
    for i in 0..2 {
        let db = env.get_handle("users").unwrap();
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
    env.flush().unwrap();
    clean_up("multi-writes");
}

#[test]
fn pair_iterator() {
    let (users, db) = insert_all_users("pair_iterator");
    let res = db.for_each::<u64, User, _>(|pair| {
        let (key, user) = pair.unwrap();
        assert_eq!(user.id, key);
        assert_eq!(users[key as usize - 1], user);
    });
    assert!(res.is_ok());
    clean_up("pair_iterator");
}

#[test]
fn exists_and_delete() {
    let (_, db) = insert_all_users("delete");
    assert!(db.contains_key(&525u64).unwrap());
    db.remove(&525u64).unwrap();
    assert_eq!(db.contains_key(&525u64).unwrap(), false);
    clean_up("delete");
}
