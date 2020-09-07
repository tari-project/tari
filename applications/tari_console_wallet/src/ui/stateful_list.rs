use tui::widgets::ListState;

pub struct StatefulList<T>
where T: Clone
{
    pub state: ListState,
    pub items: Vec<T>,
}

impl<T> StatefulList<T>
where T: Clone
{
    pub fn new() -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
            items: Vec::new(),
        }
    }

    pub fn with_items(items: Vec<T>) -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
            items,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            },
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            },
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn unselect(&mut self) {
        self.state.select(None);
    }

    pub fn select_first(&mut self) {
        self.state.select(Some(0));
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.state.selected().and_then(|i| self.items.get(i))
    }
}
