# tee 

A small adapter object to let you both write to stderr/stdout *and* capture that output.

See the docs on `tee::Tee` for more details on how it works.

## Adding to a project

```toml
# Cargo.toml
[dependencies]
tee = { git = "https://github.com/nataliejameson/tee", tag = "0.1.0" }
```

## Example Usage

```rust
fn main() {
    let tee_err = Tee::new(std::io::stderr())?;
    let res = std::process::Command::new(["sh"])
        .args(["-c", "echo \"fail\" >&2; exit 1;"])
        .stderr(tee_err.clone())
        .output()?
        .unwrap();
    assert_eq!(0, res.stderr.len());
    let stderr_out = String::from_utf8(tee_err.get_output().unwrap()).unwrap();
    assert_eq!("fail\n", stderr_out);
}
```
