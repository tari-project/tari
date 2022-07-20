// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var createError = require("http-errors");
var express = require("express");
var path = require("path");
var cookieParser = require("cookie-parser");
var logger = require("morgan");

var indexRouter = require("./routes/index");
var contractRouter = require("./routes/contract");
var validatorRouter = require("./routes/validatorNode");
var dataRouter = require("./routes/data");
var livereload = require("livereload");
var connectLiveReload = require("connect-livereload");
const liveReloadServer = livereload.createServer();
liveReloadServer.server.once("connection", () => {
  setTimeout(() => {
    liveReloadServer.refresh("/");
  }, 100);
});

var hbs = require("hbs");
const { title } = require("process");
hbs.registerHelper("hex", function (buffer) {
  return buffer ? Buffer.from(buffer).toString("hex") : "";
});
hbs.registerHelper("concat", function (string1, string2) {
  return string1 + string2;
});
hbs.registerHelper("times", function (n, block) {
  var accum = "";
  for (var i = 0; i < n; ++i) accum += block.fn({ index: i, render: `render-${i}` });
  return accum;
});
hbs.registerHelper("table", function (block) {
  let { id, endpoint, rows } = block.hash;
  let table_navigation = `<div class="navigation"><span id="${id}Prev" class="prev clickable">&lt Prev</span><span id="${id}Pages" class="pages"></span><span id="${id}Next" class="next clickable">Next &gt</span></div>`;
  let header = block.fn(block.hash);
  let cols = (header.match(/<\/th>/g) || []).length;
  let row_data = "";
  for (let i = 0; i < rows; ++i) {
    row_data += "<tr>";
    for (let j = 0; j < cols; ++j) {
      row_data += `<td id="${id}-td-${i}-${j}"></td>`;
    }
    row_data += "</tr>";
  }
  return `<table id="${id}-table" endpoint="${endpoint}" cols=${cols} rows=${rows}><thead><tr>${header}</tr></thead><tbody id="${id}Body">${row_data}</tbody></table>${table_navigation}`;
});
hbs.registerPartials(path.join(__dirname, "components"));
// hbs.registerHelper("sort", function (list, ))

var app = express();
app.use(connectLiveReload());

// view engine setup
app.set("views", path.join(__dirname, "views"));
app.set("view engine", "hbs");

app.use(logger("dev"));
app.use(express.json());
app.use(express.urlencoded({ extended: false }));
app.use(cookieParser());
app.use(express.static(path.join(__dirname, "public")));

app.use("/", indexRouter);
app.use("/validator_node", validatorRouter);
app.use("/contract", contractRouter);
app.use("/data", dataRouter);

// catch 404 and forward to error handler
app.use(function (req, res, next) {
  next(createError(404));
});

// error handler
app.use(function (err, req, res, next) {
  // set locals, only providing error in development
  res.locals.message = err.message;
  res.locals.error = req.app.get("env") === "development" ? err : {};

  // render the error page
  res.status(err.status || 500);
  res.render("error");
});

module.exports = app;
