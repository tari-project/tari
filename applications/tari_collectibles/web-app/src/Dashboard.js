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

import { Container, Grid, Button } from "@mui/material";
import React from "react";
import { AssetCard, Spinner } from "./components";
import binding from "./binding";
import { toHexString } from "./helpers";

const explorerUrl = (blockHash) =>
  `https:://explore.tari.com/block/${blockHash.toString("hex")}`;

class DashboardContent extends React.Component {
  constructor(props) {
    super(props);

    this.state = {
      error: null,
      isLoading: false,
      assets: [],
    };
  }

  async componentDidMount() {
    this.setState({
      isLoading: true,
    });

    try {
      let outputs = await binding.command_assets_list_registered_assets(0, 100);
      this.setState({
        // TODO: Fetch asset metadata from somewhere
        assets: outputs.map((o) => ({
          name: toHexString(o.unique_id),
          description: `Asset registration at block #${
            o.mined_height
          } (${toHexString(o.mined_in_block)})`,
          public_key: o.asset_public_key ? toHexString(o.asset_public_key) : "",
          image_url: "https://source.unsplash.com/random",
        })),
        isLoading: false,
      });
    } catch (err) {
      console.error(err);
      this.setState({
        error: "Could not load assets:" + err,
        isLoading: false,
      });
    }
  }

  renderTokens() {
    const { assets, isLoading } = this.state;
    if (!isLoading && assets.length === 0) {
      return <div>No assets found.</div>;
    }

    return this.state.assets.map((asset) => {
      const actions = (
        <Button
          size="small"
          to={`/view/${(asset.unique_id || "").toString("hex")}`}
        >
          View
        </Button>
      );

      return (
        <Grid item key={asset.public_key} xs={12} sm={6} md={4}>
          <AssetCard asset={asset} actions={actions} />
        </Grid>
      );
    });
  }

  render() {
    const { isLoading, error } = this.state;

    return (
      <Container maxWidth="md" sx={{ mt: 4, mb: 4, py: 8 }}>
        <Grid container spacing={4}>
          {this.renderTokens()}
          {isLoading ? <Spinner /> : <span />}
          {error ? <p>{error}</p> : <span />}
        </Grid>
      </Container>
    );
  }
}

export default DashboardContent;
