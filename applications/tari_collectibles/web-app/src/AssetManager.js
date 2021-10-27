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

import React, { useState } from "react";
import { Box, Button, Container, TextField, Typography } from "@mui/material";
import { useParams, withRouter } from "react-router-dom";
import PropTypes from "prop-types";
import binding from "./binding";
import { fs, path } from "@tauri-apps/api";

class AssetManagerContent extends React.Component {
  constructor(props) {
    super(props);

    this.state = {
      error: "",
      loading: true,
      saving: false,
      numTokens: 0,
    };

    this.onNumTokensToIssueChanged = this.onNumTokensToIssueChanged.bind(this);
    this.onIssueTokens = this.onIssueTokens.bind(this);
  }

  async componentDidMount() {
    this.setState({ loading: false });
  }

  onNumTokensToIssueChanged(e) {
    this.setState({ numTokens: e.target.value });
  }

  async onIssueTokens() {
    console.log("About to issue tokens");
    this.setState({ saving: true, error: "" });
    // Issue

    try {
      let res = await binding.command_asset_issue_simple_tokens(
        this.props.assetPubKey,
        parseInt(this.state.numTokens)
      );
      console.log(res);
    } catch (e) {
      this.setState({ error: "Could not issue tokens:" + e });
    }

    this.setState({ saving: false });
  }

  render() {
    return (
      <Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
        <Typography>Asset: {this.props.assetPubKey}</Typography>
        <Box>
          <TextField
            id="numTokens"
            onChange={this.onNumTokensToIssueChanged}
            value={this.state.numTokens}
            type="number"
            disabled={this.state.saving}
          ></TextField>
          <Button
            id="issueTokens"
            onClick={this.onIssueTokens}
            disabled={this.state.saving}
          >
            Issue Tokens
          </Button>
        </Box>
        <Box>
          <AssetDefinition assetPubKey={this.props.assetPubKey} />
        </Box>
      </Container>
    );
  }
}

const AssetDefinition = (props) => {
  const { assetPubKey } = props;
  const [msg, setMsg] = useState("");

  const asset = {
    publicKey: assetPubKey,
    phaseTimeout: 10,
    initialCommittee: [],
    baseLayerConfirmationTime: 5,
    checkpointUniqueId: [],
    templates: [],
  };
  const contents = JSON.stringify(asset);
  async function save() {
    const filename = `${assetPubKey}.json`;
    try {
      const home = await path.homeDir();
      const filePath = `${home}${filename}`;
      await fs.writeFile({
        contents,
        path: filePath,
      });
      setMsg(`Asset definition file written to: ${filePath}`);
    } catch (e) {
      setMsg(`Error: ${e}`);
    }
  }

  return (
    <div>
      <p>Asset Definition</p>
      <p>Use this asset definition json file for your validator node</p>
      <pre>{contents}</pre>
      <Button id="download" onClick={save}>
        Save asset definition file
      </Button>
      <p>{msg}</p>
    </div>
  );
};

AssetManagerContent.propTypes = {
  assetPubKey: PropTypes.string,
};

function AssetManager() {
  const { assetPubKey } = useParams();
  return <AssetManagerContent assetPubKey={assetPubKey} />;
}

export default withRouter(AssetManager);
