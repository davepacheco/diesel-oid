use anyhow::bail;
use anyhow::Context;
use diesel::connection::SimpleConnection;
use diesel::expression::SelectableHelper;
use diesel::Connection;
use diesel::Insertable;
use diesel::PgConnection;
use diesel::QueryDsl;
use diesel::Queryable;
use diesel::RunQueryDsl;
use diesel::Selectable;
use schema::my_table;

// Define a custom SQL type for a Rust enum / SQL user-defined type.
mod sql_types {
    #[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId)]
    #[diesel(postgres_type(name = "my_enum"))]
    pub struct MyEnum;
}

// Define our simple schema that uses the enum.
mod schema {
    diesel::table! {
        my_table (a) {
            a -> Int4,
            b -> crate::sql_types::MyEnum,
        }
    }
}

#[derive(Debug, Insertable, Queryable, Selectable)]
#[diesel(table_name = my_table)]
struct MyTable {
    a: i32,
    b: MyEnum,
}

#[derive(Debug, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::sql_types::MyEnum"]
pub enum MyEnum {
    One,
    Two,
}

fn main() -> anyhow::Result<()> {
    use schema::my_table::dsl;

    // Connect to the database.
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        bail!(
            "usage: {} POSTGRESQL_URL",
            if args.is_empty() { "cmd" } else { args[0].as_str() }
        );
    }
    let url = &args[1];
    println!("connecting to: {}", url);
    let mut conn1 = PgConnection::establish(url)
        .with_context(|| format!("connecting to {:?}", url))?;
    println!("connected!");

    // Initialize a basic schema using a user-defined type.
    conn1
        .batch_execute(
            r#"
        DROP TABLE IF EXISTS my_table;
        DROP TYPE IF EXISTS my_enum;
        CREATE TYPE my_enum AS ENUM ('one', 'two');
        CREATE TABLE my_table (a INT4, b my_enum);
    "#,
        )
        .context("initializing schema")?;
    println!("initialized schema");

    // Insert a value.
    let ninserted = diesel::insert_into(dsl::my_table)
        .values(MyTable { a: 0, b: MyEnum::One })
        .execute(&mut conn1)
        .context("executing initial insert")?;
    println!("inserted rows: {}", ninserted);

    // Print the contents of the table.
    let rows = dsl::my_table
        .select(MyTable::as_select())
        .get_results(&mut conn1)
        .context("listing contents of table (1)")?;
    println!("found rows: {:?}", rows);

    // Now, mess with the schema a bit.  The end result will still be compatible
    // with the diesel schema definition (in `mod schema` above); however, the
    // underlying OIDs will have changed.
    conn1
        .batch_execute(
            r#"
        ALTER TABLE my_table DROP COLUMN b;
        DROP TYPE my_enum;
        CREATE TYPE my_enum AS ENUM ('one', 'two');
        ALTER TABLE my_table ADD COLUMN b my_enum DEFAULT 'one';
        "#,
        )
        .context("updating schema")?;
    println!("updated schema");

    // Try to insert another row.  This will fail because Diesel has cached the
    // OID.
    let result = diesel::insert_into(dsl::my_table)
        .values(MyTable { a: 0, b: MyEnum::One })
        .execute(&mut conn1)
        .context("second insert failed");
    match result {
        Ok(ninserted) => {
            println!("unexpectedly inserted rows: {}", ninserted);
        }
        Err(error) => {
            println!("ERROR: {:#}", error);
        }
    }

    // Let's be sure: what happens if we use a separate connection?
    println!("establishing second connection");
    let mut conn2 = PgConnection::establish(url)
        .with_context(|| format!("connecting to {:?}", url))?;
    println!("connected!");
    let result = diesel::insert_into(dsl::my_table)
        .values(MyTable { a: 0, b: MyEnum::One })
        .execute(&mut conn2)
        .context("third insert failed");
    match result {
        Ok(ninserted) => {
            println!("insert on second connection worked: {}", ninserted);
        }
        Err(error) => {
            println!("ERROR: {:#}", error);
        }
    }

    println!("contents of table after second insert (second conn):");
    let rows = dsl::my_table
        .select(MyTable::as_select())
        .get_results(&mut conn2)
        .context("listing contents of table")?;
    println!("rows: {:?}", rows);

    println!("contents of table after second insert (first conn):");
    let rows = dsl::my_table
        .select(MyTable::as_select())
        .get_results(&mut conn1)
        .context("listing contents of table")?;
    println!("rows: {:?}", rows);

    Ok(())
}
