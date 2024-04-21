use embassy_time::Duration;

pub async fn sleep_ms(ms: u64) {
    embassy_time::Timer::after(Duration::from_millis(ms)).await;
}
