impl super::AlfenDriver {
    pub(crate) async fn calculate_excess_pv_power(&self, ev_power_w: f64) -> Option<f32> {
        let dbus_guard = self.dbus.as_ref()?.lock().await;
        async fn get_f64(svc: &crate::dbus::DbusService, path: &str) -> f64 {
            match svc
                .read_remote_value("com.victronenergy.system", path)
                .await
            {
                Ok(v) => v
                    .as_f64()
                    .or_else(|| v.as_i64().map(|x| x as f64))
                    .or_else(|| v.as_u64().map(|x| x as f64))
                    .unwrap_or(0.0),
                Err(_) => 0.0,
            }
        }
        let dc_pv = get_f64(&dbus_guard, "/Dc/Pv/Power").await;
        let ac_pv_l1 = get_f64(&dbus_guard, "/Ac/PvOnOutput/L1/Power").await;
        let ac_pv_l2 = get_f64(&dbus_guard, "/Ac/PvOnOutput/L2/Power").await;
        let ac_pv_l3 = get_f64(&dbus_guard, "/Ac/PvOnOutput/L3/Power").await;
        let total_pv = dc_pv + ac_pv_l1 + ac_pv_l2 + ac_pv_l3;
        let cons_l1 = get_f64(&dbus_guard, "/Ac/Consumption/L1/Power").await;
        let cons_l2 = get_f64(&dbus_guard, "/Ac/Consumption/L2/Power").await;
        let cons_l3 = get_f64(&dbus_guard, "/Ac/Consumption/L3/Power").await;
        let consumption = cons_l1 + cons_l2 + cons_l3;
        let adjusted_consumption = (consumption - ev_power_w).max(0.0);
        let excess = (total_pv - adjusted_consumption).max(0.0);
        Some(excess as f32)
    }
}
