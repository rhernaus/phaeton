use super::meas::RealtimeMeasurements;

impl crate::driver::AlfenDriver {
    pub(super) async fn read_realtime_values(&mut self) -> RealtimeMeasurements {
        let socket_id = self.config.modbus.socket_slave_id;
        let addr_voltages = self.config.registers.voltages;
        let addr_currents = self.config.registers.currents;
        let addr_power = self.config.registers.power;
        let addr_energy = self.config.registers.energy;
        let addr_status = self.config.registers.status;

        let manager = self.modbus_manager.as_mut().unwrap();

        // Perform a single bulk read over the contiguous socket register range
        let start_addr = *[addr_voltages, addr_currents, addr_power, addr_energy]
            .iter()
            .min()
            .unwrap();
        let end_exclusive = {
            let v_end = addr_voltages as u32 + 6;
            let c_end = addr_currents as u32 + 6;
            let p_end = addr_power as u32 + 8;
            let e_end = addr_energy as u32 + 4;
            *[v_end, c_end, p_end, e_end].iter().max().unwrap() as u16
        };
        let bulk_count: u16 = end_exclusive.saturating_sub(start_addr);

        let t_bulk = std::time::Instant::now();
        let bulk_regs = manager
            .read_holding_registers(socket_id, start_addr, bulk_count)
            .await
            .ok();
        let bulk_ms = t_bulk.elapsed().as_millis() as u64;

        // Helper to slice a subset from the bulk read by absolute address and count
        fn slice_from_bulk(
            bulk_start: u16,
            bulk_regs: &Option<Vec<u16>>,
            addr: u16,
            count: u16,
        ) -> Option<Vec<u16>> {
            if let Some(regs) = bulk_regs.as_ref() {
                let off = (addr as i32 - bulk_start as i32) as isize;
                if off >= 0 {
                    let off = off as usize;
                    let end = off + count as usize;
                    if end <= regs.len() {
                        return Some(regs[off..end].to_vec());
                    }
                }
            }
            None
        }

        // Decode from bulk; if bulk read failed or returned too-short data, fall back to individual reads
        let mut voltages = slice_from_bulk(start_addr, &bulk_regs, addr_voltages, 6);
        let mut currents = slice_from_bulk(start_addr, &bulk_regs, addr_currents, 6);
        let mut power_regs = slice_from_bulk(start_addr, &bulk_regs, addr_power, 8);
        let mut energy_regs = slice_from_bulk(start_addr, &bulk_regs, addr_energy, 4);

        let have_all = voltages.is_some()
            && currents.is_some()
            && power_regs.is_some()
            && energy_regs.is_some();
        if !have_all {
            // Fallback path: issue the four reads separately and record timings for the chart
            let t0 = std::time::Instant::now();
            voltages = manager
                .read_holding_registers(socket_id, addr_voltages, 6)
                .await
                .ok();
            let read_voltages_ms = t0.elapsed().as_millis() as u64;

            let t1 = std::time::Instant::now();
            currents = manager
                .read_holding_registers(socket_id, addr_currents, 6)
                .await
                .ok();
            let read_currents_ms = t1.elapsed().as_millis() as u64;

            let t2 = std::time::Instant::now();
            power_regs = manager
                .read_holding_registers(socket_id, addr_power, 8)
                .await
                .ok();
            let read_powers_ms = t2.elapsed().as_millis() as u64;

            let t3 = std::time::Instant::now();
            energy_regs = manager
                .read_holding_registers(socket_id, addr_energy, 4)
                .await
                .ok();
            let read_energy_ms = t3.elapsed().as_millis() as u64;

            self.last_poll_steps
                .get_or_insert_with(Default::default)
                .read_voltages_ms = Some(read_voltages_ms);
            if let Some(ref mut steps) = self.last_poll_steps {
                steps.read_currents_ms = Some(read_currents_ms);
                steps.read_powers_ms = Some(read_powers_ms);
                steps.read_energy_ms = Some(read_energy_ms);
            }
        } else {
            // Attribute the bulk time to the voltages step for charting consistency
            self.last_poll_steps
                .get_or_insert_with(Default::default)
                .read_voltages_ms = Some(bulk_ms);
            if let Some(ref mut steps) = self.last_poll_steps {
                steps.read_currents_ms = None;
                steps.read_powers_ms = None;
                steps.read_energy_ms = None;
                steps.read_station_max_ms = None;
            }
        }

        // Status is far away; read separately
        let t_status = std::time::Instant::now();
        let status_regs = manager
            .read_holding_registers(socket_id, addr_status, 5)
            .await
            .ok();
        let read_status_ms = t_status.elapsed().as_millis() as u64;

        let voltages_triplet = Self::decode_triplet(&voltages);
        let currents_triplet = Self::decode_triplet(&currents);
        let (powers_triplet, total_power) =
            Self::decode_powers(&power_regs, &voltages_triplet, &currents_triplet);
        let energy_kwh = Self::decode_energy_kwh(&energy_regs);
        let status = Self::compute_status_from_regs(&status_regs);

        // Record timings for this segment
        if let Some(ref mut steps) = self.last_poll_steps {
            steps.read_status_ms = Some(read_status_ms);
        } else {
            // If last_poll_steps was None and not set above (shouldn't happen), set status time now
            self.last_poll_steps
                .get_or_insert_with(Default::default)
                .read_status_ms = Some(read_status_ms);
        }

        RealtimeMeasurements {
            voltages: voltages_triplet,
            currents: currents_triplet,
            powers: powers_triplet,
            total_power,
            energy_kwh,
            status,
        }
    }
}
