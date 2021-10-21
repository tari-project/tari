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
    Button, Card, CardActions, CardContent, CardMedia, Container,
    Grid,
    Typography
} from "@mui/material";

const tokens = [{
    name: "Hello world"
}];

function DashboardContent (){

    return (
<Container maxWidth="md" sx={{ mt: 4, mb: 4, py:8}}>
    <Grid container spacing={4}>
        { tokens.map((token) =>
          (<Grid item key={token} xs={12} sm={6} md={4}>
              <Card sx={{ height: '100%', display: 'flex', flexDirection: 'column'}}>
                  <CardMedia component="img" sx={{ pb: "5%"  }} image="https://source.unsplash.com/random" alt="random"></CardMedia>
                  <CardContent sx={{ flexGrox:1}}>
                      <Typography gutterBottom variant="h5" component="h2">
                          Heading
                      </Typography>
                      <Typography>
                          This is a token
                      </Typography>
                  </CardContent>
                  <CardActions>
                      <Button size="small">View</Button>
                      <Button size="small">Edit</Button>
                  </CardActions>
              </Card>
          </Grid>))

        }
    </Grid>
    </Container>)
}

export default function Dashboard() {
    return <DashboardContent />;
}