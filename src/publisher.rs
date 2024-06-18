use r2r::QosProfile;
use serde_json::json;
use rand::seq::SliceRandom;
use rand::Rng;

// for testing pupposes
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = r2r::Context::create()?;
    let mut node = r2r::Node::create(ctx, "testnode", "")?;
    let duration = std::time::Duration::from_millis(200);

    let mut timer = node.create_wall_timer(duration)?;
    let publisher = node.create_publisher::<r2r::std_msgs::msg::String>("/monitored_state", QosProfile::default())?;

    let handle = tokio::task::spawn_blocking(move || loop {
        node.spin_once(std::time::Duration::from_millis(1));
    });

    for _ in 1..=10000 {
        timer.tick().await?;

        let interfaces = vec![
            String::from("publisher"),
            String::from("subscriber"),
            String::from("server"),
        ];

        let activity = vec!(
            String::from("Active"),
            String::from("Inactive"),
        );

        // let number = 0..10;
    
        let random_interface = interfaces.choose(&mut rand::thread_rng()).unwrap();
        let random_activity = activity.choose(&mut rand::thread_rng()).unwrap();
        let mut rng = rand::thread_rng();
        let i: i32 = rng.gen_range(0..10);

        let msg = r2r::std_msgs::msg::String {
            data: json!({
                "name": format!("server {}", i),
                "interface_type": random_interface,
                "state": random_activity
            }).to_string(),
        };
        publisher.publish(&msg)?;
    }

    handle.await?;
    Ok(())
}