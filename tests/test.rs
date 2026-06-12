async fn run(pool: &PgPool) -> anyhow::Result<()> {
    let user = sqlx::query!("SELECT id, name FROM users WHERE id = $1", id)
        .fetch_one(pool)
        .await?;

    let users = query_as!(
        User,
        "SELECT id, name " + "FROM users " + "WHERE active = true",
    )
    .fetch_all(pool)
    .await?;

    let users = query_as!(
        User,
        r#"SELECT id, name
        FROM users
        WHERE active = true"#,
    )
    .fetch_all(pool)
    .await?;

    let closure = || {
        let _ = query!("SELECT now() FROM " + "dual");
    };

    Ok(())
}
