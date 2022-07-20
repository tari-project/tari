// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const gen_table_route_path = (table_name) =>
  `/:${table_name}_page(\\d+)?(-:${table_name}_sort-:${table_name}_reverse)?`;
const gen_render_params = (req, ...tables) => {
  let tables_names = [];
  let indices = [];
  let sorting = [];
  let reverse = [];
  for (const table_name of tables) {
    tables_names.push(`'${table_name}'`);
    indices.push(parseInt(req.params?.[`${table_name}_page`] ?? 0));
    sorting.push(parseInt(req.params?.[`${table_name}_sort`] || -1));
    reverse.push(req.params?.[`${table_name}_reverse`] || false);
  }
  return { tables: tables_names, indices, sorting, reverse };
};

module.exports = {
  gen_table_route_path,
  gen_render_params,
};
