
/// Wrapper type that allows to hide binary data from SQL
#[derive(Clone, Debug, QueryId, Zeroize)]
pub struct HiddenSqlBinary {
    data: Hidden<Vec<u8>>,
}

impl HiddenSqlBinary {
    fn new(data: Hidden<Vec<u8>>) -> Self {
        Self { data }
    }

    fn hidden(self) -> Hidden<Vec<u8>> {
        self.data
    }
}

impl PartialEq for HiddenSqlBinary {
    fn eq(&self, other: &Self) -> bool {
        self.data.reveal() == other.data.reveal()
    }
}

impl<DB, ST> Queryable<ST, DB> for HiddenSqlBinary
where
    DB: Backend,
    Vec<u8>: Queryable<ST, DB>,
{
    type Row = <Vec<u8> as Queryable<ST, DB>>::Row;

    fn build(row: Self::Row) -> Self {
        Self::new(Hidden::hide(<Vec<u8> as Queryable<ST, DB>>::build(row)))
    }
}

impl<DB> FromSql<Binary, DB> for HiddenSqlBinary
where
    DB: Backend,
    Vec<u8>: FromSql<Binary, DB>,
{
    fn from_sql(bytes: Option<&<DB as Backend>::RawValue>) -> diesel::deserialize::Result<Self> {
        Ok(HiddenSqlBinary {
            data: Hidden::hide(<Vec<u8>>::from_sql(bytes)?),
        })
    }
}

impl<DB> ToSql<Binary, DB> for HiddenSqlBinary
where
    DB: Backend,
    Vec<u8>: ToSql<Binary, DB>,
{
    fn to_sql<W: std::io::Write>(&self, out: &mut diesel::serialize::Output<W, DB>) -> diesel::serialize::Result {
        self.data.reveal().to_sql(out)
    }
}

impl Expression for HiddenSqlBinary {
    type SqlType = Binary;
}

impl<QS> AppearsOnTable<QS> for HiddenSqlBinary {}

impl NonAggregate for HiddenSqlBinary {}

impl<DB> QueryFragment<DB> for HiddenSqlBinary where DB: Backend {}

// impl QueryId for HiddenSQLBinary {
//     type QueryId = <Binary as QueryId>::QueryId;
// }
