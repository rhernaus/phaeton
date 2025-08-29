// Measurement helpers and types for runtime polling

pub(super) struct LineTriplet {
    pub(super) l1: f64,
    pub(super) l2: f64,
    pub(super) l3: f64,
}

pub(super) struct RealtimeMeasurements {
    pub(super) voltages: LineTriplet,
    pub(super) currents: LineTriplet,
    pub(super) powers: LineTriplet,
    pub(super) total_power: f64,
    pub(super) energy_kwh: f64,
    pub(super) status: i32,
}

impl crate::driver::AlfenDriver {
    pub(super) fn decode_triplet(regs: &Option<Vec<u16>>) -> LineTriplet {
        if let Some(v) = regs
            && v.len() >= 6
        {
            let a = crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
            let b = crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
            let c = crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
            return LineTriplet {
                l1: a,
                l2: b,
                l3: c,
            };
        }
        LineTriplet {
            l1: 0.0,
            l2: 0.0,
            l3: 0.0,
        }
    }

    pub(super) fn decode_energy_kwh(regs: &Option<Vec<u16>>) -> f64 {
        if let Some(v) = regs
            && v.len() >= 4
        {
            return crate::modbus::decode_64bit_float(&v[0..4]).unwrap_or(0.0) / 1000.0;
        }
        0.0
    }

    pub(super) fn decode_powers(
        power_regs: &Option<Vec<u16>>,
        voltages: &LineTriplet,
        currents: &LineTriplet,
    ) -> (LineTriplet, f64) {
        let (mut l1, mut l2, mut l3, mut total) = if let Some(v) = power_regs {
            if v.len() >= 8 {
                let p1 = crate::modbus::decode_32bit_float(&v[0..2]).unwrap_or(0.0) as f64;
                let p2 = crate::modbus::decode_32bit_float(&v[2..4]).unwrap_or(0.0) as f64;
                let p3 = crate::modbus::decode_32bit_float(&v[4..6]).unwrap_or(0.0) as f64;
                let pt = crate::modbus::decode_32bit_float(&v[6..8]).unwrap_or(0.0) as f64;
                let sanitize = |x: f64| if x.is_finite() { x } else { 0.0 };
                (sanitize(p1), sanitize(p2), sanitize(p3), sanitize(pt))
            } else {
                (0.0, 0.0, 0.0, 0.0)
            }
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let approx = |v: f64, i: f64| (v * i).round();
        if l1.abs() < 1.0 {
            l1 = approx(voltages.l1, currents.l1);
        }
        if l2.abs() < 1.0 {
            l2 = approx(voltages.l2, currents.l2);
        }
        if l3.abs() < 1.0 {
            l3 = approx(voltages.l3, currents.l3);
        }
        if total.abs() < 1.0 {
            total = l1 + l2 + l3;
        }

        (LineTriplet { l1, l2, l3 }, total)
    }

    pub(super) fn compute_status_from_regs(status_regs: &Option<Vec<u16>>) -> i32 {
        if let Some(v) = status_regs
            && v.len() >= 5
        {
            let s = crate::modbus::decode_string(&v[0..5], None).unwrap_or_default();
            return Self::map_alfen_status_to_victron(&s) as i32;
        }
        0
    }
}
