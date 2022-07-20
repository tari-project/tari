// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const arrow_up_full = "▲";
const arrow_up_empty = "△";
const arrow_down_full = "▼";
const arrow_down_empty = "▽";

class Table {
  constructor(name, index, sorting, reverse, tables) {
    this.name = name;
    this.max_rows = 10;
    this.data = [];
    this.current_page = index;
    this.max_page = 0;
    this.page_spread = 3;
    this.render_column = {};
    this.tables = tables;
    this.endpoint = "";
    this.cols = 0;
    if (sorting >= 0) {
      this.sortColumn = sorting;
      this.sortReverse = reverse;
    } else {
      this.sortColumn = undefined;
      this.sortReverse = undefined;
    }
  }

  async load() {
    this.data = await (await fetch(`/data${this.endpoint}`)).json();
    this.max_page = Math.floor((this.data.length - 1) / this.max_rows) + 1;
    this.current_page = Math.min(this.current_page, this.max_page - 1);
    if (this.sortColumn !== undefined) {
      this.renderSorting();
    }
  }

  generatePagesNavigation() {
    const pageNavigation = (i) => {
      clickable.push(i);
      return `<span id="${this.name}-page-${i}" class="${
        i == this.current_page ? "activePage" : "clickable"
      }">${i}</span>`;
    };
    const addOnClick = (i) => {
      if (i != this.current_page)
        document.getElementById(`${this.name}-page-${i}`).onclick = (event) => {
          this.onPageClick(i);
        };
    };
    let clickable = [];
    let pages = "";
    if (this.current_page > this.page_spread) {
      pages += `${pageNavigation(0)} `;
      if (this.current_page > this.page_spread + 1) {
        pages += "... ";
      }
    }
    for (let i = this.current_page - this.page_spread; i <= this.current_page + this.page_spread; ++i) {
      if (0 <= i && i < this.max_page) {
        pages += `${pageNavigation(i)} `;
      }
    }
    if (this.current_page + this.page_spread < this.max_page - 1) {
      if (this.current_page + this.page_spread + 1 < this.max_page - 1) {
        pages += "... ";
      }
      pages += `${pageNavigation(this.max_page - 1)}`;
    }
    document.getElementById(`${this.name}Pages`).innerHTML = pages;
    for (let i of clickable) {
      addOnClick(i);
    }
  }

  sort(a, b) {
    const get_value = (data, col = this.sortColumn) => {
      return eval(this[`sort-${col}`]);
    };
    const gt = (a, b, i) => {
      let x = get_value(a, i);
      let y = get_value(b, i);
      if (x === y) return false;
      return x > y || y === undefined;
      d;
    };
    if (gt(a, b)) {
      return 1;
    }
    if (gt(b, a)) {
      return -1;
    }
    for (let i = 0; i < this.cols; ++i) {
      if (gt(a, b, i)) {
        return 1;
      }
      if (gt(b, a, i)) {
        return -1;
      }
    }
    return 0;
  }

  renderSorting() {
    if (this.sortReverse) {
      let column_desc = document.getElementById(`${this.name}-column-${this.sortColumn}-desc`);
      column_desc.hidden = false;
    } else {
      let column_asc = document.getElementById(`${this.name}-column-${this.sortColumn}-asc`);
      column_asc.hidden = false;
    }
    this.data.sort((a, b) => this.sort(a, b));
    if (this.sortReverse) {
      this.data.reverse();
    }
  }

  resetSorting() {
    if (this.sortColumn !== undefined) {
      if (this.sortReverse) {
        let column_desc = document.getElementById(`${this.name}-column-${this.sortColumn}-desc`);
        column_desc.hidden = true;
      } else {
        let column_asc = document.getElementById(`${this.name}-column-${this.sortColumn}-asc`);
        column_asc.hidden = true;
      }
    }
  }

  onColumnClick(col) {
    this.resetSorting();

    if (this.sortColumn === col) {
      this.sortReverse = !this.sortReverse;
    } else {
      this.sortReverse = false;
    }
    this.sortColumn = col;
    this.renderSorting();
    this.render();
  }

  setup() {
    let table = document.getElementById(`${this.name}-table`);
    let table_body = document.getElementById(`${this.name}Body`);
    this.cols = parseInt(table.getAttribute("cols"));
    for (let i = 0; document.getElementById(`${this.name}-column-${i}`); ++i) {
      let column = document.getElementById(`${this.name}-column-${i}`);
      column.onclick = (event) => {
        this.onColumnClick(i);
      };
      this[`render-${i}`] = column.getAttribute("render");
      this[`sort-${i}`] = column.getAttribute("for_sort") || this[`render-${i}`];

      column.innerHTML = `
      <span style="display:flex;justify-content:center;align-items:center">
        ${column.innerHTML}
        <span style="display:grid;font-size:0.5em;margin-left:0.5em">
          <span style="position:relative;top:-1em">
            <span style="position:absolute">
              ${arrow_up_empty}
            </span>
            <span id="${this.name}-column-${i}-asc" style="position:absolute" hidden=true>
              ${arrow_up_full}
            </span>
          </span>
          <span style="position:relative">
            <span style="position:absolute">
              ${arrow_down_empty}
            </span>
            <span id="${this.name}-column-${i}-desc" style="position:absolute" hidden=true>
              ${arrow_down_full}
            </span>
          </span>
        </span>
      </span>`;
    }
    this.endpoint = table.getAttribute("endpoint");
    document.getElementById(`${this.name}Prev`).onclick = (event) => {
      this.onPrevClick();
    };
    document.getElementById(`${this.name}Next`).onclick = (event) => {
      this.onNextClick();
    };
  }

  render() {
    this.generatePagesNavigation();
    for (let i = 0; i < this.max_rows; ++i) {
      for (let col = 0; col < this.cols; ++col) {
        let td = document.getElementById(`${this.name}-td-${i}-${col}`);
        let j = i + this.max_rows * this.current_page;
        if (this.data[j]) {
          if (this.cols > 1) {
            let data = this.data[j];
            console.log(data);
            console.log(this[`render-${col}`]);
            td.innerHTML = eval(this[`render-${col}`]);
            console.log("BUG?");
          } else {
            let data = this.data[j];
            td.innerHTML = eval(this[`render-${col}`]);
          }
        } else {
          td.innerHTML = "<br/>";
        }
      }
    }
    window.history.replaceState(
      {
        ...window.history.state,
        tables: {
          ...window.history.state?.["tables"],
          [this.name]: { page: this.current_page, sortColumn: this.sortColumn, sortReverse: this.sortReverse },
        },
      },
      ""
    );
    this.tables.changeUrl();
  }

  onPrevClick() {
    if (this.current_page > 0) {
      --this.current_page;
      this.render();
    }
  }

  onPageClick(i) {
    this.current_page = i;
    this.render();
  }

  onNextClick() {
    if (this.current_page < this.max_page - 1) {
      ++this.current_page;
      this.render();
    }
  }
  onElementClick(element) {
    window.location.href = `${this.name}/${element}`;
  }
}

class Tables {
  constructor(tables, indices, sortings, reverses, url) {
    this.tables = {};
    this.url = url;
    for (const [table, index, sorting, reverse] of tables.map((k, i) => [k, indices[i], sortings[i], reverses[i]])) {
      this.tables[table] = new Table(table, index, sorting, reverse, this);
      this.tables[table].setup();
    }
  }

  render() {
    Object.entries(this.tables).forEach(([key, val]) => val.render());
  }

  async load() {
    await Promise.all(Object.entries(this.tables).map(([key, val]) => val.load()));
  }

  changeUrl() {
    let url = this.url;
    for (const table_name in this.tables) {
      let table = window.history.state?.["tables"]?.[table_name];
      if (table !== undefined) {
        url += `/${table.page}`;
        if (table.sortColumn !== undefined) {
          url += `-${table.sortColumn}-${table.sortReverse}`;
        }
      }
    }
    window.history.replaceState(window.history.state, "", url);
  }
}

async function initTables(tables, indices, sorting, reverse, url) {
  tables = new Tables(tables, indices, sorting, reverse, url);
  await tables.load();
  tables.render();
}
