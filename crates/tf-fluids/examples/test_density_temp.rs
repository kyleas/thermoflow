//! Test rfluids density-temperature API

use rfluids::prelude::*;
use rfluids::substance::Pure;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test if rfluids supports density-temperature inputs
    let fluid = Fluid::from(Pure::Nitrogen);

    // Try density-temperature input
    let rho = 10.0; // kg/m3
    let t = 300.0; // K

    // Check available FluidInput methods
    println!("Testing FluidInput options for rfluids 0.3");

    // Try mass-density + temperature
    match fluid.in_state(FluidInput::density(rho), FluidInput::temperature(t)) {
        Ok(mut f) => {
            println!("✓ Density-Temperature input supported!");
            println!("  Pressure: {} Pa", f.pressure()?);
            println!("  Enthalpy: {} J/kg", f.enthalpy()?);
        }
        Err(e) => println!("✗ Density-Temperature failed: {}", e),
    }

    Ok(())
}
