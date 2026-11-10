use bt_engine::{circuit_f64_exact, circuit_f64_poly, SignSpec};
use bt_market::{equilibrium_price, monotone_coupling, Grid};
use bt_protocol::{run_session, RunConfig, SessionParams};
use clap::{Parser, Subcommand};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bt-cli", about = "Certified Blind Transport native harness")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the full in-process protocol on the demo books.
    Run,
    /// Write JSON test vectors.
    GenVectors {
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run => {
            let mut rng = ChaCha20Rng::from_seed([1u8; 32]);
            let params = SessionParams::demo([9u8; 32]);
            let a = vec![0, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
            let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
            let res = run_session(&params, &a, &b, &RunConfig::default(), &mut rng);
            println!(
                "accepted={} pi={} g_t={} welfare={} messages={} bytes={}",
                res.verdict.accepted,
                res.verdict.pi,
                res.verdict.g_t,
                res.verdict.welfare,
                res.messages.len(),
                res.bytes_total
            );
            println!(
                "honesty real={:?} simulated={:?} warnings={:?}",
                res.honesty.real, res.honesty.simulated, res.honesty.warnings
            );
            for (name, us) in &res.timings_us {
                println!("  timing {name}: {us} us");
            }
            for c in &res.verdict.checks {
                println!("  check {}: {:?} {}", c.id, c.status, c.label);
            }
        }
        Cmd::GenVectors { out } => {
            std::fs::create_dir_all(&out).ok();
            let g = Grid::DEMO;
            let a = vec![0u64, 5000, 10000, 25000, 30000, 20000, 15000, 10000];
            let b = vec![10000, 15000, 25000, 30000, 20000, 8000, 0, 0];
            let t = monotone_coupling(&g, &a, &b);
            let pi = equilibrium_price(&g, &a, &b);
            let exact = circuit_f64_exact(&g, &a, &b);
            let poly = circuit_f64_poly(&g, &a, &b, &SignSpec::fast());
            let doc = serde_json::json!({
                "grid": { "levels": g.levels, "top_ticks": g.top_ticks },
                "book_a": a,
                "book_b": b,
                "trades": t,
                "pi": pi,
                "exact_circuit": exact,
                "poly_circuit": poly,
                "security": "toy, insecure",
            });
            let path = out.join("preset.json");
            std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
            println!("wrote {}", path.display());
        }
    }
}
