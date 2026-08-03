use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("usage: memory_migration <facts.db>"))?;
    let plan = omegon_memory::sqlite::SqliteBackend::plan_migration(&path)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    if !plan.is_applicable() {
        anyhow::bail!("migration plan failed source verification");
    }
    if std::env::args().any(|arg| arg == "--apply") {
        let result = omegon_memory::sqlite::SqliteBackend::apply_migration(&plan)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Ok(())
}
