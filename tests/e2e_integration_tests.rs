use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

/// End-to-end integration tests that test the full flow:
/// Control Plane → Envoy → Test Backend
///
/// These tests require Docker Compose to be running with:
/// - control-plane service
/// - envoy service  
/// - test-backend service

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_full_routing_flow
async fn e2e_test_full_routing_flow() {
    // Wait for services to be ready
    wait_for_services().await;

    // Step 1: Create a cluster via Control Plane REST API
    let cluster_response = create_cluster_via_api("test-backend", "test-backend", 80).await;
    assert!(
        cluster_response.is_ok(),
        "Failed to create cluster: {:?}",
        cluster_response
    );

    // Step 2: Create a route via Control Plane REST API
    let route_response = create_route_via_api("/status/200", "test-backend").await;
    assert!(
        route_response.is_ok(),
        "Failed to create route: {:?}",
        route_response
    );

    // Step 3: Wait for Envoy to get the configuration
    sleep(Duration::from_secs(5)).await;

    // Step 4: Test request through Envoy to Backend
    let proxy_response = send_request_through_envoy("/status/200").await;
    assert!(
        proxy_response.is_ok(),
        "Failed to send request through Envoy: {:?}",
        proxy_response
    );
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_load_balancing_policies
async fn e2e_test_load_balancing_policies() {
    wait_for_services().await;

    // Test creating cluster with different LB policies
    let policies = vec!["ROUND_ROBIN", "LEAST_REQUEST", "RANDOM"];

    for policy in policies {
        let cluster_name = format!("test-cluster-{}", policy.to_lowercase());
        let result = create_cluster_with_lb_policy(&cluster_name, "test-backend", 80, policy).await;
        assert!(
            result.is_ok(),
            "Failed to create cluster with policy {}: {:?}",
            policy,
            result
        );

        // Verify cluster was created
        let clusters = list_clusters().await.unwrap();
        assert!(
            clusters.contains(&cluster_name),
            "Cluster {} not found in list",
            cluster_name
        );
    }
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_invalid_lb_policy
async fn e2e_test_invalid_lb_policy() {
    wait_for_services().await;

    // Try to create cluster with invalid LB policy
    let result =
        create_cluster_with_lb_policy("invalid-cluster", "test-backend", 80, "INVALID_POLICY")
            .await;
    assert!(
        result.is_err(),
        "Expected error for invalid LB policy, but got success"
    );

    let error = result.unwrap_err();
    assert!(
        error.contains("Invalid load balancing policy"),
        "Error should mention invalid policy: {}",
        error
    );
}

// Helper functions for E2E testing

async fn wait_for_services() {
    // Wait for control plane to be ready
    for _ in 0..30 {
        if health_check_control_plane().await.is_ok() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    // Wait a bit more for Envoy to connect
    sleep(Duration::from_secs(5)).await;
}

async fn health_check_control_plane() -> Result<(), String> {
    let client = reqwest::Client::new();
    match client.get("http://localhost:8080/health").send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                Err(format!("Health check failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("Health check request failed: {}", e)),
    }
}

async fn create_cluster_via_api(name: &str, host: &str, port: u16) -> Result<(), String> {
    let client = reqwest::Client::new();
    let cluster_data = json!({
        "name": name,
        "endpoints": [{"host": host, "port": port}]
    });

    match client
        .post("http://localhost:8080/clusters")
        .json(&cluster_data)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!(
                    "Create cluster failed with status: {} body: {}",
                    status, text
                ))
            }
        }
        Err(e) => Err(format!("Create cluster request failed: {}", e)),
    }
}

async fn create_cluster_with_lb_policy(
    name: &str,
    host: &str,
    port: u16,
    lb_policy: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let cluster_data = json!({
        "name": name,
        "endpoints": [{"host": host, "port": port}],
        "lb_policy": lb_policy
    });

    match client
        .post("http://localhost:8080/clusters")
        .json(&cluster_data)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                // Check if the JSON response indicates an error
                if body.contains("\"success\":false")
                    || body.contains("Invalid load balancing policy")
                {
                    Err(body)
                } else {
                    Ok(())
                }
            } else {
                let body = response.text().await.unwrap_or_default();
                Err(body)
            }
        }
        Err(e) => Err(format!("Create cluster request failed: {}", e)),
    }
}

async fn create_route_via_api(path: &str, cluster_name: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let route_data = json!({
        "path": path,
        "cluster_name": cluster_name
    });

    match client
        .post("http://localhost:8080/routes")
        .json(&route_data)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                Err(format!("Create route failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("Create route request failed: {}", e)),
    }
}

async fn send_request_through_envoy(path: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:10000{}", path);

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                Err(format!("Request failed with status: {}", status))
            }
        },
        Err(e) => Err(format!("Request to Envoy failed: {}", e)),
    }
}

async fn list_clusters() -> Result<String, String> {
    let client = reqwest::Client::new();
    match client.get("http://localhost:8080/clusters").send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                response
                    .text()
                    .await
                    .map_err(|e| format!("Failed to read clusters: {}", e))
            } else {
                Err(format!("List clusters failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("List clusters request failed: {}", e)),
    }
}

