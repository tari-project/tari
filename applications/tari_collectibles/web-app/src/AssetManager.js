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
      assetDefinition: {}
    };
  }

  async componentDidMount() {
    const { assetPubKey } = this.props;
    let registration = await binding.command_assets_get_registration(assetPubKey);
    console.log("reigstration:", registration);
    let assetDefinition = {
      public_key: assetPubKey,
      initialCommittee: registration.initialCommitee,
      checkpointUniqueId: registration.checkpointUniqueId,
      template_parameters: registration.features.template_parameters
    }
    this.setState({ loading: false, assetDefinition });
  }



  render() {
    return (
      <Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
        <Typography>Asset: {this.props.assetPubKey}</Typography>

        <Box>
          <AssetDefinition assetPubKey={this.props.assetPubKey} assetDefinition={this.state.assetDefinition} />
        </Box>
      </Container>
    );
  }
}

const AssetDefinition = (props) => {
  const { assetPubKey, assetDefinition } = props;
  const [msg, setMsg] = useState("");

  const contents = JSON.stringify(assetDefinition,null, 2);
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
      <p>Use this asset definition json file for your validator nodes</p>
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
