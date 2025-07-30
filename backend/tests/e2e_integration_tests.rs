#![allow(clippy::uninlined_format_args)]

use axum::http::header::WARNING;
use envoy_control_plane::api::handlers::get_route;
use serde_json::{json, Value};
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

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_cluster_update_endpoints
async fn e2e_test_cluster_update_endpoints() {
    wait_for_services().await;

    let cluster_name = "update-test-cluster";

    // Step 1: Create initial cluster with one endpoint
    let result = create_cluster_via_api(cluster_name, "initial-host.com", 8080).await;
    assert!(
        result.is_ok(),
        "Failed to create initial cluster: {:?}",
        result
    );
    // After the LB policy update, add a small delay
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    // Step 2: Update cluster with multiple endpoints
    let update_result = update_cluster_endpoints(
        cluster_name,
        vec![
            ("updated-host1.com", 80),
            ("updated-host2.com", 80),
            ("updated-host3.com", 8080),
        ],
    )
    .await;
    assert!(
        update_result.is_ok(),
        "Failed to update cluster endpoints: {:?}",
        update_result
    );

    // Step 3: Verify the cluster has the updated endpoints
    let cluster_details = get_cluster(cluster_name).await.unwrap();
    assert!(
        cluster_details.contains("updated-host1.com"),
        "Cluster should contain updated-host1.com"
    );
    assert!(
        cluster_details.contains("updated-host2.com"),
        "Cluster should contain updated-host2.com"
    );
    assert!(
        cluster_details.contains("updated-host3.com"),
        "Cluster should contain updated-host3.com"
    );
    assert!(
        !cluster_details.contains("initial-host.com"),
        "Cluster should not contain old endpoint"
    );

    // Step 4: Cleanup
    let _ = delete_cluster(cluster_name).await;
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_cluster_update_lb_policy
async fn e2e_test_cluster_update_lb_policy() {
    wait_for_services().await;

    let cluster_name = "lb-policy-update-cluster";

    // Step 1: Create cluster with ROUND_ROBIN (default)
    let result = create_cluster_via_api(cluster_name, "test-backend", 80).await;
    assert!(
        result.is_ok(),
        "Failed to create initial cluster: {:?}",
        result
    );

    // Step 2: Update to LEAST_REQUEST
    let update_result = update_cluster_lb_policy(cluster_name, "LEAST_REQUEST").await;
    assert!(
        update_result.is_ok(),
        "Failed to update cluster LB policy: {:?}",
        update_result
    );

    // Step 3: Verify policy was updated
    let cluster_details = get_cluster(cluster_name).await.unwrap();
    assert!(
        cluster_details.contains("LeastRequest") || cluster_details.contains("LEAST_REQUEST"),
        "Cluster should use LEAST_REQUEST policy"
    );

    // Step 4: Update to RANDOM
    let update_result = update_cluster_lb_policy(cluster_name, "RANDOM").await;
    assert!(
        update_result.is_ok(),
        "Failed to update cluster to RANDOM: {:?}",
        update_result
    );

    // Step 5: Verify policy was updated again
    let cluster_details = get_cluster(cluster_name).await.unwrap();
    assert!(
        cluster_details.contains("Random") || cluster_details.contains("RANDOM"),
        "Cluster should use RANDOM policy"
    );

    // Step 6: Cleanup
    let _ = delete_cluster(cluster_name).await;
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_cluster_update_nonexistent
async fn e2e_test_cluster_update_nonexistent() {
    wait_for_services().await;

    // Try to update a cluster that doesn't exist
    let result = update_cluster_endpoints("nonexistent-cluster", vec![("some-host.com", 80)]).await;

    assert!(
        result.is_err(),
        "Expected error when updating nonexistent cluster"
    );
    let error = result.unwrap_err();
    assert!(
        error.contains("404") || error.contains("not found"),
        "Should get 404 error for nonexistent cluster: {}",
        error
    );
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_cluster_full_lifecycle
async fn e2e_test_cluster_full_lifecycle() {
    wait_for_services().await;

    let cluster_name = "lifecycle-test-cluster";

    // Step 1: Create cluster
    let result =
        create_cluster_with_lb_policy(cluster_name, "initial.com", 80, "ROUND_ROBIN").await;
    assert!(result.is_ok(), "Failed to create cluster: {:?}", result);

    // Step 2: Update endpoints
    let update_result = update_cluster_endpoints(
        cluster_name,
        vec![("endpoint1.com", 80), ("endpoint2.com", 8080)],
    )
    .await;

    print!("Waiting 100ms");
    // After the LB policy update, add a small delay
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(
        update_result.is_ok(),
        "Failed to update endpoints: {:?}",
        update_result
    );

    // Step 3: Update load balancing policy
    let lb_update_result = update_cluster_lb_policy(cluster_name, "LEAST_REQUEST").await;
    assert!(
        lb_update_result.is_ok(),
        "Failed to update LB policy: {:?}",
        lb_update_result
    );

    // Step 4: Verify final state
    let cluster_details = get_cluster(cluster_name).await.unwrap();
    println!("DEBUG: Cluster details response: {}", cluster_details);
    assert!(
        cluster_details.contains("endpoint1.com"),
        "Should contain endpoint1.com. Got: {}",
        cluster_details
    );
    assert!(
        cluster_details.contains("endpoint2.com"),
        "Should contain endpoint2.com"
    );
    assert!(
        cluster_details.contains("LeastRequest") || cluster_details.contains("LEAST_REQUEST"),
        "Should use LEAST_REQUEST policy"
    );

    // Step 5: Delete cluster
    let delete_result = delete_cluster(cluster_name).await;
    assert!(
        delete_result.is_ok(),
        "Failed to delete cluster: {:?}",
        delete_result
    );

    // Step 6: Verify cluster is gone
    let get_result = get_cluster(cluster_name).await;
    assert!(
        get_result.is_err(),
        "Cluster should not exist after deletion"
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
        }
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

async fn get_cluster(name: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/clusters/{}", name);
    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                response
                    .text()
                    .await
                    .map_err(|e| format!("Failed to read cluster: {}", e))
            } else {
                Err(format!("Get cluster failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("Get cluster request failed: {}", e)),
    }
}

async fn update_cluster_endpoints(name: &str, endpoints: Vec<(&str, u16)>) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/clusters/{}", name);

    let endpoints_json: Vec<_> = endpoints
        .into_iter()
        .map(|(host, port)| json!({"host": host, "port": port}))
        .collect();

    let update_data = json!({
        "endpoints": endpoints_json
    });

    println!(
        "DEBUG: Sending PUT request to {} with data: {}",
        url, update_data
    );
    match client.put(&url).json(&update_data).send().await {
        Ok(response) => {
            let status = response.status();
            let response_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response".to_string());
            println!(
                "DEBUG: PUT response status: {}, body: {}",
                status, response_text
            );
            if status.is_success() {
                Ok(())
            } else {
                Err(format!(
                    "Update cluster endpoints failed with status: {} body: {}",
                    status, response_text
                ))
            }
        }
        Err(e) => Err(format!("Update cluster endpoints request failed: {}", e)),
    }
}

async fn update_cluster_lb_policy(name: &str, lb_policy: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/clusters/{}", name);

    // First get the current cluster to keep existing endpoints
    let current_cluster = get_cluster(name).await?;

    // // Parse current endpoints (simplified - assumes response contains host:port patterns)
    let cluster_data: Value = serde_json::from_str(&current_cluster)
        .map_err(|e| format!("Failed to parse cluster data: {}", e))?;
    let endpoints_json = cluster_data["data"]["endpoints"].clone();

    let update_data = json!({
        "endpoints": endpoints_json,
        "lb_policy": lb_policy
    });

    match client.put(&url).json(&update_data).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!(
                    "Update cluster LB policy failed with status: {} body: {}",
                    status, text
                ))
            }
        }
        Err(e) => Err(format!("Update cluster LB policy request failed: {}", e)),
    }
}

async fn delete_cluster(name: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/clusters/{}", name);

    match client.delete(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!(
                    "Delete cluster failed with status: {} body: {}",
                    status, text
                ))
            }
        }
        Err(e) => Err(format!("Delete cluster request failed: {}", e)),
    }
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_route_update_http_methods
async fn e2e_test_route_update_http_methods() {
    wait_for_services().await;

    let cluster_name = "route-update-test-cluster";
    let route_path = "/status/202";  // httpbin endpoint that works with all methods - unique path

    // Step 1: Create a cluster and initial route with GET method only
    let cluster_result = create_cluster_via_api(cluster_name, "test-backend", 80).await;
    assert!(cluster_result.is_ok(), "Failed to create cluster: {:?}", cluster_result);

    let route_result = create_route_with_methods(route_path, cluster_name, Some(vec!["GET"])).await;
    assert!(route_result.is_ok(), "Failed to create route: {:?}", route_result);

    // Get the route ID for updates
    let route_id = get_route_id_by_path(route_path).await.unwrap();

    // Wait for Envoy to get the configuration
    sleep(Duration::from_secs(5)).await;

    // Step 2: Test initial GET request works
    let get_response = send_request_through_envoy_with_method(route_path, "GET").await;
    assert!(get_response.is_ok(), "GET request should succeed: {:?}", get_response);

    // Step 3: Test POST request fails (not allowed initially)
    let post_response = send_request_through_envoy_with_method(route_path, "POST").await;
    match &post_response {
        Ok(_) => println!("POST request succeeded with status 200. The path was {route_path}"),
        Err(e) => println!("POST request failed: {}", e),
    };

    
    assert!(post_response.is_err(), "POST request should fail initially");

    // Step 4: Update route to allow GET and POST methods
    let update_result = update_route_methods(&route_id, route_path, cluster_name, vec!["GET", "POST"]).await;
    assert!(update_result.is_ok(), "Failed to update route methods: {:?}", update_result);

    // Wait for Envoy to get the updated configuration
    sleep(Duration::from_secs(5)).await;

    // Step 5: Test both GET and POST now work
    let get_response = send_request_through_envoy_with_method(route_path, "GET").await;
    assert!(get_response.is_ok(), "GET request should still work: {:?}", get_response);

    let post_response = send_request_through_envoy_with_method(route_path, "POST").await;
    assert!(post_response.is_ok(), "POST request should now work: {:?}", post_response);

    // Step 6: Update route to remove method restrictions (allow all)
    let update_result = update_route_remove_methods(&route_id, route_path, cluster_name).await;
    assert!(update_result.is_ok(), "Failed to remove method restrictions: {:?}", update_result);

    // Wait for Envoy to get the updated configuration
    sleep(Duration::from_secs(5)).await;

    // Step 7: Test that PUT method now works (should be allowed with no restrictions)
    let put_response = send_request_through_envoy_with_method(route_path, "PUT").await;
    assert!(put_response.is_ok(), "PUT request should work with no restrictions: {:?}", put_response);

    // Cleanup
    let _ = delete_route(&route_id).await;
    let _ = delete_cluster(cluster_name).await;
}

#[tokio::test]
#[ignore] // Run with: cargo test --ignored e2e_test_route_update_full_lifecycle  
async fn e2e_test_route_update_full_lifecycle() {
    wait_for_services().await;

    let cluster_name = "lifecycle-route-cluster";
    let initial_path = "/status/203";  // httpbin endpoint that works with all methods - unique path
    let updated_path = "/status/204";  // httpbin endpoint that works with all methods - unique path

    // Step 1: Create cluster and initial route
    let cluster_result = create_cluster_via_api(cluster_name, "test-backend", 80).await;
    assert!(cluster_result.is_ok(), "Failed to create cluster: {:?}", cluster_result);

    let route_result = create_route_with_methods(initial_path, cluster_name, Some(vec!["GET"])).await;
    assert!(route_result.is_ok(), "Failed to create route: {:?}", route_result);

    let route_id = get_route_id_by_path(initial_path).await.unwrap();

    let route_response = get_route_by_id(&route_id).await;
    println!("DEBUG: Route response after update: {:?}", route_response);

    // Wait for Envoy configuration
    sleep(Duration::from_secs(5)).await;

    // Step 2: Test initial route works
    let initial_response = send_request_through_envoy_with_method(initial_path, "GET").await;
    assert!(initial_response.is_ok(), "Initial route should work: {:?}", initial_response);

    // Step 3: Update route to new path and different methods
    let update_result = update_route_full(&route_id, updated_path, cluster_name, vec!["POST", "PUT"]).await;
    assert!(update_result.is_ok(), "Failed to update route: {:?}", update_result);

    // Wait for Envoy configuration
    sleep(Duration::from_secs(5)).await;

    let route_response = get_route_by_id(&route_id).await;
    println!("DEBUG: Route response after update: {:?}", route_response);

    // Step 4: Test old path no longer works
    let old_response = send_request_through_envoy_with_method(initial_path, "GET").await;
    assert!(old_response.is_err(), "Old route path should no longer work");

    // Step 5: Test new path works with correct methods
    let new_post_response = send_request_through_envoy_with_method(updated_path, "POST").await;
    assert!(new_post_response.is_ok(), "New route POST should work: {:?}", new_post_response);

    let new_put_response = send_request_through_envoy_with_method(updated_path, "PUT").await;
    assert!(new_put_response.is_ok(), "New route PUT should work: {:?}", new_put_response);

    // Step 6: Test new path rejects old method
    let new_get_response = send_request_through_envoy_with_method(updated_path, "GET").await;
    assert!(new_get_response.is_err(), "New route should reject GET method");

    // Cleanup
    let _ = delete_route(&route_id).await;
    let _ = delete_cluster(cluster_name).await;
}

// Helper functions for route testing

async fn create_route_with_methods(path: &str, cluster_name: &str, methods: Option<Vec<&str>>) -> Result<(), String> {
    let client = reqwest::Client::new();
    let mut route_data = json!({
        "path": path,
        "cluster_name": cluster_name
    });

    if let Some(methods) = methods {
        route_data["http_methods"] = json!(methods);
    }

    match client
        .post("http://localhost:8080/routes")
        .json(&route_data)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                println!("DEBUG: Create route response status: {} and body: {}", status, &route_data);
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!("Create route failed with status: {} body: {}", status, text))
            }
        }
        Err(e) => Err(format!("Create route request failed: {}", e)),
    }
}

async fn get_route_by_id(route_id: &str) -> Result<Value, String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/routes/{}", route_id);
    
    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                let text = response.text().await.unwrap_or_default();
                serde_json::from_str(&text).map_err(|e| format!("Failed to parse route response: {}", e))
            } else {
                Err(format!("Get route failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("Get route request failed: {}", e)),
    }
}

async fn get_route_id_by_path(path: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    match client.get("http://localhost:8080/routes").send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                let text = response.text().await.unwrap_or_default();
                let routes_response: Value = serde_json::from_str(&text)
                    .map_err(|e| format!("Failed to parse routes response: {}", e))?;
                
                if let Some(routes) = routes_response["data"].as_array() {
                    for route in routes {
                        if route["path"].as_str() == Some(path) {
                            if let Some(id) = route["id"].as_str() {
                                return Ok(id.to_string());
                            }
                        }
                    }
                }
                Err(format!("Route with path {} not found", path))
            } else {
                Err(format!("Get routes failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("Get routes request failed: {}", e)),
    }
}

async fn update_route_methods(route_id: &str, path: &str, cluster_name: &str, methods: Vec<&str>) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/routes/{}", route_id);
    
    let update_data = json!({
        "path": path,
        "cluster_name": cluster_name,
        "http_methods": methods
    });

    match client.put(&url).json(&update_data).send().await {
        Ok(response) => {
            let status = response.status();
            println!("DEBUG: Update route {path} methods response status: {status}");
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!("Update route methods failed with status: {} body: {}", status, text))
            }
        }
        Err(e) => Err(format!("Update route methods request failed: {}", e)),
    }
}

async fn update_route_remove_methods(route_id: &str, path: &str, cluster_name: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/routes/{}", route_id);
    
    let update_data = json!({
        "path": path,
        "cluster_name": cluster_name
    });

    match client.put(&url).json(&update_data).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!("Update route remove methods failed with status: {} body: {}", status, text))
            }
        }
        Err(e) => Err(format!("Update route remove methods request failed: {}", e)),
    }
}

async fn update_route_full(route_id: &str, path: &str, cluster_name: &str, methods: Vec<&str>) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/routes/{}", route_id);
    
    let update_data = json!({
        "path": path,
        "cluster_name": cluster_name,
        "http_methods": methods
    });

    match client.put(&url).json(&update_data).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!("Update route full failed with status: {} body: {}", status, text))
            }
        }
        Err(e) => Err(format!("Update route full request failed: {}", e)),
    }
}

async fn send_request_through_envoy_with_method(path: &str, method: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:10000{}", path);

    let request = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        "HEAD" => client.head(&url),
        _ => return Err(format!("Unsupported HTTP method: {}", method)),
    };

    match request.send().await {
        Ok(response) => {
            let status = response.status();
            println!("DEBUG: Request to {} with method {} returned status: {}", url, method, status);
            if status.is_success() {
                Ok(())
            } else {
                Err(format!("Request failed with status: {}", status))
            }
        }
        Err(e) => Err(format!("Request to Envoy failed: {}", e)),
    }
}

async fn delete_route(route_id: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("http://localhost:8080/routes/{}", route_id);

    match client.delete(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                Ok(())
            } else {
                let text = response.text().await.unwrap_or_default();
                Err(format!("Delete route failed with status: {} body: {}", status, text))
            }
        }
        Err(e) => Err(format!("Delete route request failed: {}", e)),
    }
}
