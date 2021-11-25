//  Copyright 2021. The Tari Project
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

import {
  Container,
  Button,
  TextField,
  Stack,
  Typography,
  FormGroup, Alert,
} from "@mui/material";
import React, { useState, useEffect } from "react";
import { Spinner } from "./components";
import binding from "./binding";
import { withRouter, useParams } from "react-router-dom";

const chunk = (arr, len) => {
  const chunks = [];
  let i = 0;
  let n = arr.length;

  while (i < n) {
    chunks.push(arr.slice(i, (i += len)));
  }

  return chunks;
};

const SeedWords = ({ wallet, password, history }) => {
  const [seedWords, setSeedWords] = useState([]);
  const [error, setError] = useState("");
  useEffect(() => {
    binding
      .command_wallets_seed_words(wallet.id, password)
      .then((words) => setSeedWords(words))
      .catch((e) => {

        console.error("error: ", e);
        setError(e.message);
      });
  }, [wallet.id, password]);

  const display = (seedWords) => {
    console.log(seedWords);
    if (seedWords.length === 0) return <Spinner />;

    const chunks = chunk(seedWords, 6);
    return (
      <div>
        {chunks.map((words, i) => (
          <pre key={i}>{words.join(" ")}</pre>
        ))}
      </div>
    );
  };

  return (
    <div>
      <Typography variant="h3" sx={{ mb: "30px" }}>
        Seed words
      </Typography>
      {error ? (
          <Alert severity="error">{error}</Alert>
      ) : (
          <span />
      )}
      <p>
        Save these seed words securely. This is the recovery phrase for this
        wallet.
      </p>
      {display(seedWords)}
      <Button
        disabled={seedWords.length === 0}
        onClick={() => history.push(`/dashboard`)}
      >
        I have saved my seed words
      </Button>
    </div>
  );
};

const CreateWallet = ({ history }) => {
  const [password, setPassword] = useState("");
  const [password2, setPassword2] = useState("");
  const [creating, setCreating] = useState(false);
  const [wallet, setWallet] = useState(undefined);
  const [error, setError] = useState("");

  if (wallet)
    return <SeedWords wallet={wallet} password={password} history={history} />;

  let valid = false;
  let helperText = "Passwords must match.";
  if (password === password2) {
    valid = true;
    helperText = "";
  }

  const create = async () => {
    setCreating(true);
    try {
      const wallet = await binding.command_wallets_create(password, "main");
      console.log("wallet", wallet);
      setWallet(wallet);
    }
    catch(err) {
      setError(err.message);
    }
  };

  return (
    <div>
      <Typography variant="h3" sx={{ mb: "30px" }}>
        Create new wallet
      </Typography>
      <Stack>
        {error ? (
            <Alert severity="error">{error}</Alert>
        ) : (
            <span />
        )}
        <FormGroup>
          <TextField
            id="password"
            type="password"
            label="Password"
            variant="filled"
            color="primary"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          ></TextField>
          <TextField
            id="password2"
            type="password"
            label="Confirm Password"
            variant="filled"
            color="primary"
            value={password2}
            helperText={helperText}
            error={!valid}
            onChange={(e) => setPassword2(e.target.value)}
          ></TextField>
          <Button disabled={!valid || creating} onClick={create}>
            Create wallet
          </Button>
        </FormGroup>
      </Stack>
    </div>
  );
};

const OpenWallet = ({ history, setAuthenticated }) => {
  const { id } = useParams();
  const [password, setPassword] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const [error, setError] = useState("");
  const isError = error !== "";

  const unlock = async () => {
    setUnlocking(true);
    try {
      await binding.command_wallets_unlock(id, password);
      setAuthenticated(id, password);
      history.push("/dashboard");
    } catch (e) {
      console.error("error: ", e);
      setUnlocking(false);
      setError(e.message);
    }
  };

  return (
    <Container maxWidth="md" sx={{ mt: 4, mb: 4, py: 8 }}>
      <Typography variant="h3" sx={{ mb: "30px" }}>
        Unlock wallet
      </Typography>
      <Stack>
        <FormGroup>
          <TextField
            id="password"
            type="password"
            label="Password"
            variant="filled"
            color="primary"
            value={password}
            error={isError}
            helperText={error}
            onChange={(e) => {
              setPassword(e.target.value);
              setError("");
            }}
          ></TextField>
          <Button disabled={unlocking} onClick={unlock}>
            Unlock
          </Button>
        </FormGroup>
      </Stack>
    </Container>
  );
};

const Setup = ({ history }) => {
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    binding
      .command_wallets_list()
      .then((wallets) => {
        console.log("wallets", wallets);
        if (wallets.length > 0) {
          const wallet = wallets[0];
          history.push(`/wallets/${wallet.id}`);
        } else {
          setLoading(false);
        }
      })
      .catch((e) => {
        // todo error handling
        console.error("wallets_list error:", e);
      });
  }, [history]);

  if (loading) return <Spinner />;

  return (
    <Container maxWidth="md" sx={{ mt: 4, mb: 4, py: 8 }}>
      <CreateWallet history={history} />
    </Container>
  );
};

const UnlockWallet = withRouter(OpenWallet);
export { UnlockWallet };
export default withRouter(Setup);
