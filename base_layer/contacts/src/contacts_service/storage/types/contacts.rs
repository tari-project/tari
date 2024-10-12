// Copyright 2023. The Tari Project
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

use std::convert::TryFrom;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use tari_common_sqlite::util::diesel_ext::ExpectedRowsExtension;
use tari_common_types::tari_address::TariAddress;
use tari_network::ToPeerId;

use crate::{
    contacts_service::{error::ContactsServiceStorageError, types::Contact},
    schema::contacts,
};

/// A Sql version of the Contact struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq, Eq)]
#[diesel(table_name = contacts)]
pub struct ContactSql {
    pub address: Vec<u8>,
    node_id: Vec<u8>,
    pub alias: String,
    last_seen: Option<NaiveDateTime>,
    latency: Option<i32>,
    pub favourite: i32,
}

impl ContactSql {
    /// Write this struct to the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), ContactsServiceStorageError> {
        diesel::insert_into(contacts::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    /// Return all contacts
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<ContactSql>, ContactsServiceStorageError> {
        Ok(contacts::table.load::<ContactSql>(conn)?)
    }

    /// Find a particular Contact by their address, if it exists
    pub fn find_by_address(
        address: &[u8],
        conn: &mut SqliteConnection,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        Ok(contacts::table
            .filter(contacts::address.eq(address))
            .first::<ContactSql>(conn)?)
    }

    /// Find a particular Contact by their node ID, if it exists
    pub fn find_by_node_id(
        node_id: &[u8],
        conn: &mut SqliteConnection,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        Ok(contacts::table
            .filter(contacts::node_id.eq(node_id))
            .first::<ContactSql>(conn)?)
    }

    /// updates the address field of the contact
    pub fn set_address_of_node_id(
        node_id: &[u8],
        address: &[u8],
        conn: &mut SqliteConnection,
    ) -> Result<(), ContactsServiceStorageError> {
        diesel::update(contacts::table.filter(contacts::node_id.eq(node_id)))
            .set(contacts::address.eq(address))
            .execute(conn)?;
        Ok(())
    }

    /// Find a particular Contact by their address, and delete it if it exists, returning the affected record
    pub fn find_by_address_and_delete(
        conn: &mut SqliteConnection,
        address: &[u8],
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        let contact = ContactSql::find_by_address(address, conn)?;
        if diesel::delete(contacts::table.filter(contacts::address.eq(address))).execute(conn)? == 0 {
            return Err(ContactsServiceStorageError::ValuesNotFound);
        }
        Ok(contact)
    }

    /// Find a particular Contact by their node ID, and update it if it exists, returning the affected record
    pub fn find_by_node_id_and_update(
        conn: &mut SqliteConnection,
        node_id: &[u8],
        updated_contact: UpdateContact,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        diesel::update(contacts::table.filter(contacts::node_id.eq(node_id)))
            .set(updated_contact)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        ContactSql::find_by_node_id(node_id, conn)
    }

    /// Find a particular Contact by their node ID, and delete it if it exists, returning the affected record
    pub fn find_by_node_id_and_delete(
        conn: &mut SqliteConnection,
        node_id: &[u8],
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        let contact = ContactSql::find_by_node_id(node_id, conn)?;
        diesel::delete(contacts::table.filter(contacts::node_id.eq(node_id)))
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        Ok(contact)
    }
}

/// Conversion from an Contact to the Sql datatype form
impl TryFrom<ContactSql> for Contact {
    type Error = ContactsServiceStorageError;

    #[allow(clippy::cast_sign_loss)]
    fn try_from(o: ContactSql) -> Result<Self, Self::Error> {
        let address = TariAddress::from_bytes(&o.address).map_err(|_| ContactsServiceStorageError::ConversionError)?;
        Ok(Self {
            // Public key must always be the master data source for node ID here
            node_id: address.public_spend_key().to_peer_id(),
            address,
            alias: o.alias,
            last_seen: o.last_seen,
            latency: o.latency.map(|val| val as u32),
            favourite: match o.favourite {
                0 => false,
                1 => true,
                _ => return Err(ContactsServiceStorageError::ConversionError),
            },
        })
    }
}

/// Conversion from a Contact to the Sql datatype form
#[allow(clippy::cast_possible_wrap)]
impl From<Contact> for ContactSql {
    fn from(o: Contact) -> Self {
        Self {
            // Public key must always be the master data source for node ID here
            node_id: o.address.public_spend_key().to_peer_id().to_bytes(),
            address: o.address.to_vec(),
            alias: o.alias,
            last_seen: o.last_seen,
            latency: o.latency.map(|val| val as i32),
            favourite: i32::from(o.favourite),
        }
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = contacts)]
pub struct UpdateContact {
    pub alias: Option<String>,
    pub last_seen: Option<Option<NaiveDateTime>>,
    pub latency: Option<Option<i32>>,
    pub favourite: Option<i32>,
}
