// tf-core/src/units.rs

use uom::si::f64::{
    Acceleration as UomAcceleration, Area as UomArea, DynamicViscosity as UomDynamicViscosity,
    Energy as UomEnergy, Length as UomLength, Mass as UomMass, MassDensity as UomMassDensity,
    MassRate as UomMassRate, Power as UomPower, Pressure as UomPressure, Ratio as UomRatio,
    TemperatureInterval as UomTemperatureInterval,
    ThermodynamicTemperature as UomThermodynamicTemperature, Time as UomTime,
    Velocity as UomVelocity,
};

// Public canonical unit types (SI, f64)
pub type Accel = UomAcceleration;
pub type Area = UomArea;
pub type DynVisc = UomDynamicViscosity;
pub type Energy = UomEnergy;
pub type Length = UomLength;
pub type Mass = UomMass;
pub type Density = UomMassDensity;
pub type MassRate = UomMassRate;
pub type Power = UomPower;
pub type Pressure = UomPressure;
pub type Ratio = UomRatio;
pub type TempInterval = UomTemperatureInterval;
pub type Temperature = UomThermodynamicTemperature;
pub type Time = UomTime;
pub type Velocity = UomVelocity;

#[inline]
pub fn pa(v: f64) -> Pressure {
    use uom::si::pressure::pascal;
    Pressure::new::<pascal>(v)
}

#[inline]
pub fn k(v: f64) -> Temperature {
    use uom::si::thermodynamic_temperature::kelvin;
    Temperature::new::<kelvin>(v)
}

#[inline]
pub fn kgps(v: f64) -> MassRate {
    use uom::si::mass_rate::kilogram_per_second;
    MassRate::new::<kilogram_per_second>(v)
}

#[inline]
pub fn m(v: f64) -> Length {
    use uom::si::length::meter;
    Length::new::<meter>(v)
}

#[inline]
pub fn s(v: f64) -> Time {
    use uom::si::time::second;
    Time::new::<second>(v)
}

#[inline]
pub fn unitless(v: f64) -> Ratio {
    use uom::si::ratio::ratio;
    Ratio::new::<ratio>(v)
}

pub mod constants {
    use super::*;

    pub const G0_MPS2: f64 = 9.806_65;

    #[inline]
    pub fn g0() -> Accel {
        use uom::si::acceleration::meter_per_second_squared;
        Accel::new::<meter_per_second_squared>(G0_MPS2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_smoke() {
        let _p = pa(101_325.0);
        let _t = k(300.0);
        let _mdot = kgps(1.2);
        let _l = m(2.0);
        let _dt = s(0.1);
        let _r = unitless(0.5);
        let _g0 = constants::g0();
    }
}
