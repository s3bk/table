#[cfg(feature="datasize")]
use datasize::DataSize;

use log::{error, debug};

#[derive(Clone, Debug)]
#[cfg_attr(feature="datasize", derive(DataSize))]
#[cfg_attr(feature="serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature="rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
pub enum Cell<T> {
    Empty,
    Occupied { value: T, colspan: u32, rowspan: u32 },
    Shadowed { col: u32, row: u32 }
}

#[derive(Debug)]
pub enum Error {
    Shadowed { col: u32, row: u32 },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::Shadowed { col, row } => write!(f, "Shadowd by cell at row {row}, col {col}")
        }
    }
}
impl std::error::Error for Error {

}

#[derive(Clone, Debug)]
#[cfg_attr(feature="datasize", derive(DataSize))]
#[cfg_attr(feature="serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature="rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
pub struct Table<T> {
    num_cols: u32,
    num_rows: u32,
    cells: Vec<Cell<T>>
}
use std::mem::replace;
use std::ops::{Index, IndexMut};
use std::fmt;
use std::collections::HashSet;

impl<T> Table<T> {
    pub fn new() -> Self {
        Table { num_cols: 0, num_rows: 0, cells: vec![] }
    }
    pub fn empty(rows: u32, columns: u32) -> Self {
        let cells = std::iter::from_fn(|| Some(Cell::Empty)).take(rows as usize * columns as usize).collect();
        Table { num_cols: columns, num_rows: rows, cells }
    }
    pub fn size(&self) -> (u32, u32) {
        (self.num_rows, self.num_cols)
    }
    pub fn set_cell(&mut self, value: T, row: u32, col: u32, rowspan: u32, colspan: u32) -> Result<Option<T>, Error> {
        let cols = col + colspan;
        let rows = row + rowspan;
        if cols > self.num_cols {
            let cells = replace(&mut self.cells, Vec::with_capacity((cols as usize) * (self.num_rows as usize).max(row as usize + 1)));
            let mut cells = cells.into_iter();
            for _ in 0 .. self.num_rows {
                self.cells.extend(cells.by_ref().take(self.num_cols as usize));
                self.cells.extend(std::iter::from_fn(|| Some(Cell::Empty)).take(cols as usize - self.num_cols as usize));
            }
            self.num_cols = cols;
            
            assert_eq!(self.num_cols as usize * self.num_rows as usize, self.cells.len());
        }
        if rows > self.num_rows {
            self.cells.extend(std::iter::from_fn(|| Some(Cell::Empty)).take((rows - self.num_rows) as usize * self.num_cols as usize));
            self.num_rows = rows;

            assert_eq!(self.num_cols as usize * self.num_rows as usize, self.cells.len());
        }
        let new_cell = Cell::Occupied { value, colspan, rowspan };
        let old_cell = self.replace(row, col, new_cell);
        match old_cell {
            Cell::Occupied { value: cell_value, colspan: old_colspan, rowspan: old_rowspan } => {
                for r in row + 1 .. row + old_rowspan {
                    self.set(r, col, Cell::Empty);
                }
                for c in col + 1 .. col + old_colspan {
                    for r in row .. row + old_rowspan {
                        self.set(r, c, Cell::Empty);
                    }
                }

                for r in row + 1 .. row + rowspan {
                    self.set(r, col, Cell::Shadowed { col, row });
                }
                for c in col .. col + colspan {
                    for r in row .. row + rowspan {
                        self.set(r, c, Cell::Shadowed { col, row });
                    }
                }
                Ok(Some(cell_value))
            }
            Cell::Empty => {
                for r in row + 1 .. row + rowspan {
                    self.set(r, col, Cell::Shadowed { col, row });
                }
                for c in col + 1 .. col + colspan {
                    for r in row .. row + rowspan {
                        self.set(r, c, Cell::Shadowed { col, row });
                    }
                }
                Ok(None)
            }
            Cell::Shadowed { col, row } => Err(Error::Shadowed { col, row }),
        }
    }
    #[inline]
    fn cell_index(&self, row: u32, col: u32) -> usize {
        self.num_cols as usize * row as usize + col as usize
    }
    #[inline]
    fn set(&mut self, row: u32, col: u32, value: Cell<T>) {
        let index = self.cell_index(row, col);
        if let Some(cell) = self.cells.get_mut(index) {
            *cell = value;
        } else {
            panic!("cell row={row}, col={col} out of bounds");
        }
    }
    #[inline]
    fn replace(&mut self, row: u32, col: u32, value: Cell<T>) -> Cell<T> {
        let index = self.cell_index(row, col);
        if let Some(cell) = self.cells.get_mut(index) {
            replace(cell, value)
        } else {
            panic!("cell row={row}, col={col} out of bounds");
        }
    }
    pub fn get_cell_value_mut(&mut self, row: u32, col: u32) -> Option<&mut T> {
        let idx = self.cell_index(row, col);
        match self.cells.get_mut(idx) {
            Some(&mut Cell::Occupied { ref mut value, .. }) => Some(value),
            Some(&mut Cell::Shadowed { col, row }) => {
                debug!("shadowed cell at row={row}, col={col}");
                None
            }
            Some(&mut Cell::Empty) => {
                debug!("no data at row={row}, col={col}");
                None
            }
            None => {
                debug!("out of bounds row={row}, col={col}");
                None
            }
        }
    }
    pub fn format_html<W: fmt::Write>(&self, w: &mut W, format_cell: impl Fn(&mut W, &T) -> fmt::Result) -> fmt::Result {
        assert_eq!(self.num_cols as usize * self.num_rows as usize, self.cells.len());
        if self.num_cols == 0 || self.num_rows == 0 {
            return Ok(());
        }
        writeln!(w, "<table>")?;
        writeln!(w, "<tbody>")?;
        for row in self.cells.chunks_exact(self.num_cols as usize) {
            writeln!(w, "<tr>")?;
            for cell in row {
                match *cell {
                    Cell::Empty => write!(w, "<td></td>")?,
                    Cell::Occupied { ref value, colspan, rowspan } => {
                        write!(w, "<td")?;
                        if colspan != 1 {
                            write!(w, " colspan={}", colspan)?;
                        }
                        if rowspan != 1 {
                            write!(w, " rowspan={}", rowspan)?;
                        }
                        write!(w, ">")?;
                        format_cell(w, value)?;
                        writeln!(w, "</td>")?;
                    }
                    Cell::Shadowed { .. } => {}
                }
            }
            writeln!(w, "</tr>")?
        }
        writeln!(w, "</tbody>")?;
        writeln!(w, "</table>")?;
        Ok(())
    }
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Table<U> {
        Table {
            num_cols: self.num_cols,
            num_rows: self.num_rows,
            cells: self.cells.into_iter().map(|cell| match cell {
                Cell::Empty => Cell::Empty,
                Cell::Occupied { value, colspan, rowspan } => Cell::Occupied { value: f(value), colspan, rowspan },
                Cell::Shadowed { col, row } => Cell::Shadowed { col, row }
            }).collect()
        }
    }
    pub fn flat_map<U>(&self, mut f: impl FnMut(&T) -> Option<U>) -> Table<U> {
        let mut deleted = HashSet::new();
        Table {
            num_cols: self.num_cols,
            num_rows: self.num_rows,
            cells: self.cells_iter().map(|(row, col, cell)| match *cell {
                Cell::Empty => Cell::Empty,
                Cell::Occupied { ref value, colspan, rowspan } => match f(value) {
                    Some(value) => Cell::Occupied { value, colspan, rowspan },
                    None => {
                        if colspan != 1 && rowspan != 1 {
                            deleted.insert((col, row));
                        }
                        Cell::Empty
                    }
                }
                Cell::Shadowed { col, row } if deleted.contains(&(col, row)) => Cell::Empty,
                Cell::Shadowed { col, row } => Cell::Shadowed { col, row },
            }).collect()
        }
    }
    pub fn values(&self) -> impl Iterator<Item=CellValue<T>> {
        self.cells_iter().flat_map(|(row, col, cell)| match *cell {
            Cell::Occupied { ref value, colspan, rowspan } => Some(CellValue {
                value,
                row, col,
                rowspan, colspan
            }),
            _ => None
        })
    }
    fn cells_iter(&self) -> impl Iterator<Item=(u32, u32, &Cell<T>)> {
        self.cells.chunks_exact(self.num_cols as usize).enumerate().
        flat_map(|(row, cells)|
            cells.iter().enumerate().map(move |(col, cell)| (row as u32, col as u32, cell))
        )
    }
}
pub struct CellValue<'a, T> {
    pub value: &'a T,
    pub col: u32,
    pub row: u32,
    pub colspan: u32,
    pub rowspan: u32,
}
