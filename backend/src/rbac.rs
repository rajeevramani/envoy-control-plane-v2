use casbin::{Enforcer, CoreApi, RbacApi, MgmtApi, Result as CasbinResult, DefaultModel, FileAdapter, MemoryAdapter};
use std::sync::Arc;
use tokio::sync::RwLock;

/// RBAC Enforcer - handles all permission checking
/// Uses Arc<RwLock<>> for thread-safe access in multi-threaded Axum environment
#[derive(Clone)]
pub struct RbacEnforcer {
    enforcer: Arc<RwLock<Enforcer>>,
}

impl RbacEnforcer {
    /// Create new RBAC enforcer from policy files
    pub async fn new(model_path: String, policy_path: String) -> CasbinResult<Self> {
        println!("🔐 Loading RBAC model from: {model_path}" );
        println!("📋 Loading RBAC policies from: {policy_path}" );
        
        // Create model and adapter from file paths
        let model = DefaultModel::from_file(&model_path).await?;
        let adapter = FileAdapter::new(policy_path);
        let enforcer = Enforcer::new(model, adapter).await?;
        
        println!("✅ RBAC system initialized successfully!");
        Ok(Self {
            enforcer: Arc::new(RwLock::new(enforcer)),
        })
    }
    
    /// Create a simple in-memory RBAC enforcer with basic policies
    pub async fn new_simple() -> anyhow::Result<Self> {
        println!("🔐 Creating simple in-memory RBAC enforcer...");
        
        // Simple RBAC model as a string
        let model_text = r#"
[request_definition]
r = sub, obj, act

[policy_definition]
p = sub, obj, act

[role_definition]
g = _, _

[policy_effect]
e = some(where (p.eft == allow))

[matchers]
m = g(r.sub, p.sub) && r.obj == p.obj && r.act == p.act
"#;

        // Create model from text
        let model = DefaultModel::from_str(model_text).await?;
        let adapter = MemoryAdapter::default();
        let mut enforcer = Enforcer::new(model, adapter).await?;
        
        // Add some basic policies
        // Admin can do everything
        enforcer.add_policy(vec!["admin".to_string(), "routes".to_string(), "read".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "routes".to_string(), "write".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "routes".to_string(), "delete".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "clusters".to_string(), "read".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "clusters".to_string(), "write".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "clusters".to_string(), "delete".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "config".to_string(), "generate".to_string()]).await?;
        enforcer.add_policy(vec!["admin".to_string(), "system".to_string(), "read".to_string()]).await?;

        enforcer.add_policy(vec!["api_developer".to_string(), "routes".to_string(), "read".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "routes".to_string(), "write".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "routes".to_string(), "update".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "routes".to_string(), "delete".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "clusters".to_string(), "read".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "clusters".to_string(), "write".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "clusters".to_string(), "delete".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "config".to_string(), "generate".to_string()]).await?;
        enforcer.add_policy(vec!["api_developer".to_string(), "system".to_string(), "read".to_string()]).await?;
        
        
        // User can only read
        enforcer.add_policy(vec!["user".to_string(), "routes".to_string(), "read".to_string()]).await?;
        enforcer.add_policy(vec!["user".to_string(), "clusters".to_string(), "read".to_string()]).await?;
        enforcer.add_policy(vec!["user".to_string(), "system".to_string(), "read".to_string()]).await?;
        
        // Assign some default roles (these would normally come from a database/config)
        enforcer.add_grouping_policy(vec!["admin".to_string(), "admin".to_string()]).await?;
        enforcer.add_grouping_policy(vec!["user".to_string(), "user".to_string()]).await?;
        enforcer.add_grouping_policy(vec!["developer".to_string(), "api_developer".to_string()]).await?;
        
        println!("✅ Simple RBAC system initialized with default policies!");
        Ok(Self {
            enforcer: Arc::new(RwLock::new(enforcer)),
        })
    }
    
    /// Check if a user can perform an action on a resource
    pub async fn check_permission(
        &self,
        user_id: &str,
        resource: &str, 
        action: &str,
    ) -> CasbinResult<bool> {
        println!("🔍 RBAC Check: user={user_id}, resource={resource}, action={action}");
        
        let enforcer = self.enforcer.read().await;
        let allowed = enforcer.enforce((user_id, resource, action))?;
        
        println!("✅ RBAC Result: {}", if allowed { "ALLOWED" } else { "DENIED" });
        Ok(allowed)
    }
    
    /// Add a user to a role (e.g., "john" → "admin")
    pub async fn assign_role(&self, user_id: &str, role: &str) -> CasbinResult<bool> {
        println!("👤 Assigning role '{role}' to user '{user_id}'" );
        let mut enforcer = self.enforcer.write().await;
        enforcer.add_grouping_policy(vec![user_id.to_string(), role.to_string()]).await
    }
    
    /// Remove a user from a role
    pub async fn remove_role(&self, user_id: &str, role: &str) -> CasbinResult<bool> {
        println!("🗑️ Removing role '{role}' from user '{user_id}'" );
        let mut enforcer = self.enforcer.write().await;
        enforcer.remove_grouping_policy(vec![user_id.to_string(), role.to_string()]).await
    }
    
    /// Get all roles for a user
    pub async fn get_user_roles(&self, user_id: &str) -> Vec<String> {
        let enforcer = self.enforcer.read().await;
        let roles = enforcer.get_roles_for_user(user_id, None);
        println!("🔍 RBAC: get_user_roles('{user_id}') = {roles:?}");
        roles
    }
    
    /// Get all users with a specific role
    pub async fn get_users_for_role(&self, role: &str) -> Vec<String> {
        let enforcer = self.enforcer.read().await;
        enforcer.get_users_for_role(role, None)
    }
    
    /// Get user permissions organized by resource
    pub async fn get_user_permissions(&self, user_id: &str) -> std::collections::HashMap<String, Vec<String>> {
        let mut permissions = std::collections::HashMap::new();
        let resources = ["routes", "clusters", "config", "system", "auth"];
        let actions = ["read", "write", "delete", "generate", "login", "logout"];
        
        for resource in &resources {
            let mut allowed_actions = Vec::new();
            for action in &actions {
                if let Ok(allowed) = self.check_permission(user_id, resource, action).await {
                    if allowed {
                        allowed_actions.push(action.to_string());
                    }
                }
            }
            if !allowed_actions.is_empty() {
                permissions.insert(resource.to_string(), allowed_actions);
            }
        }
        
        permissions
    }
}

/// Map HTTP requests to RBAC resources and actions
pub fn extract_resource_and_action(method: &str, path: &str) -> (String, String) {
    match (method, path) {
        // Routes endpoints
        ("GET", p) if p.starts_with("/routes") => ("routes".to_string(), "read".to_string()),
        ("POST", "/routes") => ("routes".to_string(), "write".to_string()),
        ("PUT", p) if p.starts_with("/routes/") => ("routes".to_string(), "write".to_string()),
        ("DELETE", p) if p.starts_with("/routes/") => ("routes".to_string(), "delete".to_string()),
        
        // Clusters endpoints  
        ("GET", p) if p.starts_with("/clusters") => ("clusters".to_string(), "read".to_string()),
        ("POST", "/clusters") => ("clusters".to_string(), "write".to_string()),
        ("PUT", p) if p.starts_with("/clusters/") => ("clusters".to_string(), "write".to_string()),
        ("DELETE", p) if p.starts_with("/clusters/") => ("clusters".to_string(), "delete".to_string()),
        
        // Config generation
        ("POST", "/generate-config") => ("config".to_string(), "generate".to_string()),
        ("GET", "/generate-bootstrap") => ("config".to_string(), "generate".to_string()),
        
        // HTTP methods endpoint (public read access)
        ("GET", "/supported-http-methods") => ("system".to_string(), "read".to_string()),
        
        // Default: treat as system access
        _ => ("system".to_string(), "read".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    
    #[tokio::test]
    async fn test_resource_extraction() {
        // Test routes
        let (resource, action) = extract_resource_and_action("GET", "/routes");
        assert_eq!(resource, "routes");
        assert_eq!(action, "read");
        
        // Test clusters  
        let (resource, action) = extract_resource_and_action("DELETE", "/clusters/test-cluster");
        assert_eq!(resource, "clusters");
        assert_eq!(action, "delete");
        
        // Test config generation
        let (resource, action) = extract_resource_and_action("POST", "/generate-config");
        assert_eq!(resource, "config");
        assert_eq!(action, "generate");
    }
    
    #[tokio::test]
    async fn test_rbac_basic_functionality() {
        // Skip test if policy files don't exist
        if !Path::new("rbac_model.conf").exists() || !Path::new("rbac_policy.csv").exists() {
            println!("⚠️ Skipping RBAC test - policy files not found");
            return;
        }
        
        let rbac = RbacEnforcer::new("rbac_model.conf".to_string(), "rbac_policy.csv".to_string()).await.unwrap();
        
        // Test that we can check permissions (actual results depend on policy file)
        let result = rbac.check_permission("admin", "routes", "read").await;
        assert!(result.is_ok(), "Permission check should not error");
    }
}