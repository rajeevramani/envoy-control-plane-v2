use envoy_control_plane::storage::models::*;
use envoy_control_plane::xds::conversion::ProtoConverter;
use prost::Message;

#[tokio::test]
async fn test_cluster_to_proto_conversion() {
    let cluster = Cluster {
        name: "test-cluster".to_string(),
        endpoints: vec![
            Endpoint {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            Endpoint {
                host: "127.0.0.1".to_string(),
                port: 8081,
            },
        ],
    };

    let proto_clusters = ProtoConverter::clusters_to_proto(vec![cluster]).unwrap();
    assert_eq!(proto_clusters.len(), 1);
    
    let proto_cluster = &proto_clusters[0];
    assert_eq!(proto_cluster.type_url, "type.googleapis.com/envoy.config.cluster.v3.Cluster");
    assert!(!proto_cluster.value.is_empty());
    
    // Verify we can decode the protobuf
    let decoded = envoy_types::pb::envoy::config::cluster::v3::Cluster::decode(&proto_cluster.value[..]).unwrap();
    assert_eq!(decoded.name, "test-cluster");
    assert!(decoded.load_assignment.is_some());
    
    let load_assignment = decoded.load_assignment.unwrap();
    assert_eq!(load_assignment.cluster_name, "test-cluster");
    assert_eq!(load_assignment.endpoints.len(), 1);
    assert_eq!(load_assignment.endpoints[0].lb_endpoints.len(), 2);
}

#[tokio::test]
async fn test_route_to_proto_conversion() {
    let route = Route {
        id: "test-id".to_string(),
        path: "/api/v1/users".to_string(),
        cluster_name: "user-service".to_string(),
        prefix_rewrite: Some("/users".to_string()),
    };

    let proto_routes = ProtoConverter::routes_to_proto(vec![route]).unwrap();
    assert_eq!(proto_routes.len(), 1);
    
    let proto_route = &proto_routes[0];
    assert_eq!(proto_route.type_url, "type.googleapis.com/envoy.config.route.v3.RouteConfiguration");
    assert!(!proto_route.value.is_empty());
    
    // Verify we can decode the protobuf
    let decoded = envoy_types::pb::envoy::config::route::v3::RouteConfiguration::decode(&proto_route.value[..]).unwrap();
    assert_eq!(decoded.name, "local_route");
    assert_eq!(decoded.virtual_hosts.len(), 1);
    
    let virtual_host = &decoded.virtual_hosts[0];
    assert_eq!(virtual_host.name, "local_service");
    assert_eq!(virtual_host.domains, vec!["*"]);
    assert_eq!(virtual_host.routes.len(), 1);
    
    let route_config = &virtual_host.routes[0];
    assert!(route_config.r#match.is_some());
    assert!(route_config.action.is_some());
}

#[tokio::test]
async fn test_empty_clusters_conversion() {
    let proto_clusters = ProtoConverter::clusters_to_proto(vec![]).unwrap();
    assert!(proto_clusters.is_empty());
}

#[tokio::test]
async fn test_empty_routes_conversion() {
    let proto_routes = ProtoConverter::routes_to_proto(vec![]).unwrap();
    assert!(proto_routes.is_empty());
}

#[tokio::test]
async fn test_multiple_routes_conversion() {
    let routes = vec![
        Route {
            id: "route1".to_string(),
            path: "/api/v1/users".to_string(),
            cluster_name: "user-service".to_string(),
            prefix_rewrite: Some("/users".to_string()),
        },
        Route {
            id: "route2".to_string(),
            path: "/api/v1/orders".to_string(),
            cluster_name: "order-service".to_string(),
            prefix_rewrite: None,
        },
    ];

    let proto_routes = ProtoConverter::routes_to_proto(routes).unwrap();
    assert_eq!(proto_routes.len(), 1); // Should create one RouteConfiguration with multiple routes
    
    let proto_route = &proto_routes[0];
    let decoded = envoy_types::pb::envoy::config::route::v3::RouteConfiguration::decode(&proto_route.value[..]).unwrap();
    assert_eq!(decoded.virtual_hosts[0].routes.len(), 2);
}

#[tokio::test]
async fn test_cluster_with_single_endpoint() {
    let cluster = Cluster {
        name: "single-endpoint-cluster".to_string(),
        endpoints: vec![
            Endpoint {
                host: "192.168.1.100".to_string(),
                port: 3000,
            },
        ],
    };

    let proto_clusters = ProtoConverter::clusters_to_proto(vec![cluster]).unwrap();
    assert_eq!(proto_clusters.len(), 1);
    
    let decoded = envoy_types::pb::envoy::config::cluster::v3::Cluster::decode(&proto_clusters[0].value[..]).unwrap();
    assert_eq!(decoded.name, "single-endpoint-cluster");
    
    let load_assignment = decoded.load_assignment.unwrap();
    assert_eq!(load_assignment.endpoints[0].lb_endpoints.len(), 1);
    
    let endpoint = &load_assignment.endpoints[0].lb_endpoints[0];
    assert!(endpoint.host_identifier.is_some());
}

#[tokio::test] 
async fn test_route_without_prefix_rewrite() {
    let route = Route {
        id: "test-id".to_string(),
        path: "/health".to_string(),
        cluster_name: "health-service".to_string(),
        prefix_rewrite: None,
    };

    let proto_routes = ProtoConverter::routes_to_proto(vec![route]).unwrap();
    let decoded = envoy_types::pb::envoy::config::route::v3::RouteConfiguration::decode(&proto_routes[0].value[..]).unwrap();
    
    let route_config = &decoded.virtual_hosts[0].routes[0];
    if let Some(envoy_types::pb::envoy::config::route::v3::route::Action::Route(route_action)) = &route_config.action {
        assert_eq!(route_action.prefix_rewrite, "");
    }
}

#[tokio::test]
async fn test_route_with_prefix_rewrite() {
    let route = Route {
        id: "test-id".to_string(),
        path: "/api/v1/health".to_string(),
        cluster_name: "health-service".to_string(),
        prefix_rewrite: Some("/health".to_string()),
    };

    let proto_routes = ProtoConverter::routes_to_proto(vec![route]).unwrap();
    let decoded = envoy_types::pb::envoy::config::route::v3::RouteConfiguration::decode(&proto_routes[0].value[..]).unwrap();
    
    let route_config = &decoded.virtual_hosts[0].routes[0];
    if let Some(envoy_types::pb::envoy::config::route::v3::route::Action::Route(route_action)) = &route_config.action {
        assert_eq!(route_action.prefix_rewrite, "/health");
    }
}

#[tokio::test]
async fn test_multiple_clusters_conversion() {
    let clusters = vec![
        Cluster {
            name: "service1".to_string(),
            endpoints: vec![
                Endpoint {
                    host: "127.0.0.1".to_string(),
                    port: 8080,
                },
            ],
        },
        Cluster {
            name: "service2".to_string(),
            endpoints: vec![
                Endpoint {
                    host: "127.0.0.1".to_string(),
                    port: 8081,
                },
            ],
        },
    ];

    let proto_clusters = ProtoConverter::clusters_to_proto(clusters).unwrap();
    assert_eq!(proto_clusters.len(), 2);
    
    let decoded1 = envoy_types::pb::envoy::config::cluster::v3::Cluster::decode(&proto_clusters[0].value[..]).unwrap();
    let decoded2 = envoy_types::pb::envoy::config::cluster::v3::Cluster::decode(&proto_clusters[1].value[..]).unwrap();
    
    assert_eq!(decoded1.name, "service1");
    assert_eq!(decoded2.name, "service2");
}