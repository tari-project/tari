//  Copyright 2022. The Tari Project
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

import React, { useState, useEffect } from "react";
import {
  Box,
  Button,
  Container,
  FormGroup,
  TextField,
  Typography,
} from "@mui/material";
import { useParams, withRouter } from "react-router-dom";
import PropTypes from "prop-types";
import binding from "./binding";
import { fs, path } from "@tauri-apps/api";

const publicKeyToRow = (publicKey, removeFn) => {
  return (
    <li key={publicKey}>
      {publicKey} <Button onClick={() => removeFn(publicKey)}>remove</Button>
    </li>
  );
};

const AssetCommitteeManager = () => {
  const { assetPublicKey } = useParams();
  const [originalCommittee, setOriginalCommittee] = useState([]);
  const [newCommittee, setNewCommittee] = useState([]);
  const [newMember, setNewMember] = useState("");

  // fetch committee
  useEffect(() => {
    binding
      .command_asset_get_committee_definition(assetPublicKey)
      .then((committee) => {
        console.log(committee);
        setOriginalCommittee(committee.sort());
        setNewCommittee(committee);
      })
      .catch((e) => console.error(e)); // todo: error
  }, [assetPublicKey]);

  const removeMember = (publicKey) => {
    const committee = newCommittee.filter((pk) => pk !== publicKey);
    setNewCommittee(committee);
  };

  const addMember = (publicKey) => {
    // todo: validate public key
    const committee = [...newCommittee, publicKey];
    setNewCommittee(committee);
    setNewMember("");
  };

  const addDisabled = (str) => {
    const s = str.trim();
    const empty = s === "";
    const wrongLength = s.length !== 64;
    const inCommittee = newCommittee.includes(s);

    return empty || wrongLength || inCommittee;
  };

  const submitDisabled = (oldComm, newComm) => {
    return (
      newComm.length === 0 ||
      JSON.stringify(oldComm) === JSON.stringify(newComm.sort())
    );
  };

  const submit = (committee) => {
    binding
      .command_asset_create_committee_definition(
        assetPublicKey,
        committee,
        false
      )
      .then((r) => {
        console.log(r);
      })
      .catch((e) => console.error(e)); // todo: error
  };

  return (
    <Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
      <Typography>Manage Committee for Asset: {assetPublicKey}</Typography>

      <p>Committee Members:</p>
      <ul>{newCommittee.map((pk) => publicKeyToRow(pk, removeMember))}</ul>

      <TextField
        label="New Member"
        value={newMember}
        onChange={(e) => setNewMember(e.target.value)}
      ></TextField>
      <Button
        onClick={() => addMember(newMember)}
        disabled={addDisabled(newMember)}
      >
        Add Member
      </Button>

      <Button
        onClick={() => {
          console.log("submit", newCommittee);
          submit(newCommittee);
        }}
        disabled={submitDisabled(originalCommittee, newCommittee)}
      >
        Save New Committee
      </Button>
    </Container>
  );
};

export default withRouter(AssetCommitteeManager);
