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
import {
  Box,
  Button,
  Container,
  FormGroup,
  TextField,
  Typography,
} from "@mui/material";
import { useParams, withRouter, useHistory } from "react-router-dom";
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
      assetDefinition: {},
    };
  }

  async componentDidMount() {
    const { assetPublicKey } = this.props;
    let registration = await binding.command_assets_get_registration(
      assetPublicKey
    );
    console.log("registration:", registration);
    let assetDefinition = {
      public_key: assetPublicKey,
      initialCommittee: registration.initialCommitee,
      checkpointUniqueId: registration.checkpointUniqueId,
      template_parameters: registration.features.template_parameters,
      templateIds: registration.features.template_ids_implemented,
    };
    this.setState({ loading: false, assetDefinition });
  }

  render() {
    return (
      <Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
        <Typography>Asset: {this.props.assetPublicKey}</Typography>

        <Box>
          <AssetDefinition
            assetPublicKey={this.props.assetPublicKey}
            assetDefinition={this.state.assetDefinition}
            manageLink={this.props.manageLink}
          />
        </Box>
      </Container>
    );
  }
}

const AssetDefinition = (props) => {
  const { assetPublicKey, assetDefinition } = props;
  const [msg, setMsg] = useState("");
  const [tip004MintTokenName, setTip004MintTokenName] = useState("Token1");
  let tip004 = false;
  let tip721 = false;
  if (assetDefinition.templateIds) {
    tip004 = assetDefinition.templateIds.includes(4);
    tip721 = assetDefinition.templateIds.includes(721);
  }
  const contents = JSON.stringify(assetDefinition, null, 2);
  async function save() {
    const filename = `${assetPublicKey}.json`;
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

  async function mint() {
    try {
      await binding.command_tip004_mint_token(
        assetPublicKey,
        tip004MintTokenName
      );
    } catch (err) {
      console.error(err);
      setMsg(`Error: ${err.message}`);
    }
  }

  return (
    <div>
      <p>Asset Definition</p>
      <p>{msg}</p>
      <p>Use this asset definition json file for your validator nodes</p>
      <pre>{contents}</pre>
      <Button id="download" onClick={save}>
        Save asset definition file
      </Button>
      <Button onClick={props.manageLink}>Manage committee</Button>

      {tip721 ? (
        <Container>
          {tip004 ? (
            <FormGroup>
              <Typography>Mint a token</Typography>
              <TextField
                id="tip004Name"
                label="Name/Description"
                value={tip004MintTokenName}
                onChange={(e) => setTip004MintTokenName(e.target.value)}
              ></TextField>
              <Button id="mint" onClick={mint}>
                Mint
              </Button>
            </FormGroup>
          ) : (
            ""
          )}
          <div>TODO: get tokens</div>
        </Container>
      ) : (
        ""
      )}
    </div>
  );
};

AssetDefinition.propTypes = {
  assetPublicKey: PropTypes.string,
  assetDefinition: PropTypes.shape({
    templateIds: PropTypes.array,
  }),
  manageLink: PropTypes.func,
};

AssetManagerContent.propTypes = {
  assetPublicKey: PropTypes.string,
  manageLink: PropTypes.func,
};

function AssetManager() {
  const { assetPublicKey } = useParams();
  const history = useHistory();
  return (
    <AssetManagerContent
      assetPublicKey={assetPublicKey}
      manageLink={() => history.push(`/assets/committee/${assetPublicKey}`)}
    />
  );
}

export default withRouter(AssetManager);
