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
import React from "react";
import {
  Alert,
  Button,
  Card,
  CardActions,
  CardContent,
  CardMedia,
  Container,
  Grid,
  Typography,
} from "@mui/material";
import binding from "./binding";
import { Link } from "react-router-dom";

var cardStyle = {
  width: "20vw",
  transitionDuration: "0.3s",
  height: "30vw",
  margin: "1vw",
  display: "block",
  flexDirection: "column",
};

class LibraryContent extends React.Component {
  constructor(props) {
    super(props);

    this.state = {
      error: "",
      ownedAssets: [],
      favoriteAssets: [
        /* todo */
      ],
    };
  }

  async componentDidMount() {
    this.setState({ error: "" });
    try {
      let assets = await binding.command_assets_list();
      console.log(assets);
      this.setState({
        ownedAssets: assets,
      });
    } catch (err) {
      console.error(err);
      this.setState({ error: "Could not load assets:" + err });
    }
  }

  render() {
    return (
      <Container maxWidth="lg" sx={{ mt: 4, mb: 4, py: 8 }}>
        {this.state.error ? (
          <Alert severity="error">{this.state.error}</Alert>
        ) : (
          ""
        )}
        <Typography variant="h3">Assets I've Issued</Typography>
        <Grid container spacing={4}>
          {this.state.ownedAssets.map((asset) => (
            <Grid item key={asset} xs={12} sm={6} md={4} lg={4}>
              <Card style={cardStyle}>
                <CardMedia
                  component="img"
                  sx={{ pb: "5%", height: "20vw", width: "20vw" }}
                  image={asset.image}
                  alt="random"
                ></CardMedia>
                <CardContent>
                  <Typography gutterBottom variant="h5" component="h2">
                    {asset.name}
                  </Typography>
                  <Typography>{asset.description}</Typography>
                </CardContent>
                <CardActions>
                  <Link to={`/assets/manage/${asset.public_key}`}>View</Link>
                  {/*<Button size="small">Edit</Button>*/}
                </CardActions>
              </Card>
            </Grid>
          ))}
        </Grid>
      </Container>
    );
  }
}

export default function Manage() {
  return <LibraryContent />;
}
