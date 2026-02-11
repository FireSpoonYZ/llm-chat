use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub transport: String,
    pub command: Option<String>,
    pub args: Option<String>,
    pub url: Option<String>,
    pub env_vars: Option<String>,
    pub is_enabled: bool,
    pub created_at: String,
}

pub async fn create_mcp_server(
    pool: &SqlitePool,
    name: &str,
    description: Option<&str>,
    transport: &str,
    command: Option<&str>,
    args: Option<&str>,
    url: Option<&str>,
    env_vars: Option<&str>,
    is_enabled: bool,
) -> McpServer {
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query_as::<_, McpServer>(
        "INSERT INTO mcp_servers (id, name, description, transport, \
         command, args, url, env_vars, is_enabled) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, name, description, transport, \
         command, args, url, env_vars, is_enabled, created_at",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(transport)
    .bind(command)
    .bind(args)
    .bind(url)
    .bind(env_vars)
    .bind(is_enabled)
    .fetch_one(pool)
    .await
    .expect("Failed to create MCP server")
}

pub async fn list_mcp_servers(pool: &SqlitePool) -> Vec<McpServer> {
    sqlx::query_as::<_, McpServer>(
        "SELECT id, name, description, transport, \
         command, args, url, env_vars, is_enabled, created_at \
         FROM mcp_servers ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to list MCP servers")
}

pub async fn list_enabled_mcp_servers(pool: &SqlitePool) -> Vec<McpServer> {
    sqlx::query_as::<_, McpServer>(
        "SELECT id, name, description, transport, \
         command, args, url, env_vars, is_enabled, created_at \
         FROM mcp_servers WHERE is_enabled = 1 \
         ORDER BY name ASC",
    )
    .fetch_all(pool)
    .await
    .expect("Failed to list enabled MCP servers")
}

pub async fn get_mcp_server(
    pool: &SqlitePool,
    id: &str,
) -> Option<McpServer> {
    sqlx::query_as::<_, McpServer>(
        "SELECT id, name, description, transport, \
         command, args, url, env_vars, is_enabled, created_at \
         FROM mcp_servers WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("Failed to get MCP server")
}

pub async fn update_mcp_server(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    description: Option<&str>,
    transport: &str,
    command: Option<&str>,
    args: Option<&str>,
    url: Option<&str>,
    env_vars: Option<&str>,
    is_enabled: bool,
) -> Option<McpServer> {
    sqlx::query_as::<_, McpServer>(
        "UPDATE mcp_servers SET name = ?, description = ?, \
         transport = ?, command = ?, args = ?, url = ?, \
         env_vars = ?, is_enabled = ? \
         WHERE id = ? \
         RETURNING id, name, description, transport, \
         command, args, url, env_vars, is_enabled, created_at",
    )
    .bind(name)
    .bind(description)
    .bind(transport)
    .bind(command)
    .bind(args)
    .bind(url)
    .bind(env_vars)
    .bind(is_enabled)
    .bind(id)
    .fetch_optional(pool)
    .await
    .expect("Failed to update MCP server")
}

pub async fn delete_mcp_server(pool: &SqlitePool, id: &str) -> bool {
    let result = sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .expect("Failed to delete MCP server");

    result.rows_affected() > 0
}

pub async fn set_conversation_mcp_servers(
    pool: &SqlitePool,
    conversation_id: &str,
    server_ids: &[String],
) {
    // Delete existing associations
    sqlx::query(
        "DELETE FROM conversation_mcp_servers WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .execute(pool)
    .await
    .expect("Failed to clear conversation MCP servers");

    // Insert new associations
    for server_id in server_ids {
        sqlx::query(
            "INSERT INTO conversation_mcp_servers \
             (conversation_id, mcp_server_id) VALUES (?, ?)",
        )
        .bind(conversation_id)
        .bind(server_id)
        .execute(pool)
        .await
        .expect("Failed to insert conversation MCP server");
    }
}

pub async fn get_conversation_mcp_servers(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Vec<McpServer> {
    sqlx::query_as::<_, McpServer>(
        "SELECT s.id, s.name, s.description, s.transport, \
         s.command, s.args, s.url, s.env_vars, s.is_enabled, s.created_at \
         FROM mcp_servers s \
         INNER JOIN conversation_mcp_servers cms \
         ON s.id = cms.mcp_server_id \
         WHERE cms.conversation_id = ? \
         ORDER BY s.name ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .expect("Failed to get conversation MCP servers")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::db::conversations::create_conversation;
    use crate::db::users::create_user;

    async fn setup() -> SqlitePool {
        init_db("sqlite::memory:").await
    }

    #[tokio::test]
    async fn test_create_mcp_server() {
        let pool = setup().await;
        let server = create_mcp_server(
            &pool,
            "test-server",
            Some("A test server"),
            "stdio",
            Some("node"),
            Some("[\"server.js\"]"),
            None,
            Some("{\"KEY\":\"val\"}"),
            true,
        )
        .await;
        assert_eq!(server.name, "test-server");
        assert_eq!(server.description.as_deref(), Some("A test server"));
        assert_eq!(server.transport, "stdio");
        assert_eq!(server.command.as_deref(), Some("node"));
        assert_eq!(server.args.as_deref(), Some("[\"server.js\"]"));
        assert!(server.url.is_none());
        assert_eq!(server.env_vars.as_deref(), Some("{\"KEY\":\"val\"}"));
        assert!(server.is_enabled);
        assert!(!server.id.is_empty());
    }

    #[tokio::test]
    async fn test_list_mcp_servers() {
        let pool = setup().await;
        create_mcp_server(&pool, "alpha", None, "stdio", None, None, None, None, true).await;
        create_mcp_server(&pool, "beta", None, "sse", None, None, Some("http://localhost"), None, false).await;
        let all = list_mcp_servers(&pool).await;
        assert_eq!(all.len(), 2);
        // Should be ordered by name ASC
        assert_eq!(all[0].name, "alpha");
        assert_eq!(all[1].name, "beta");
    }

    #[tokio::test]
    async fn test_list_enabled_mcp_servers() {
        let pool = setup().await;
        create_mcp_server(&pool, "enabled-one", None, "stdio", None, None, None, None, true).await;
        create_mcp_server(&pool, "disabled-one", None, "stdio", None, None, None, None, false).await;
        let enabled = list_enabled_mcp_servers(&pool).await;
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "enabled-one");
    }

    #[tokio::test]
    async fn test_update_mcp_server() {
        let pool = setup().await;
        let server = create_mcp_server(
            &pool, "old-name", None, "stdio", None, None, None, None, true,
        )
        .await;
        let updated = update_mcp_server(
            &pool,
            &server.id,
            "new-name",
            Some("Updated desc"),
            "sse",
            None,
            None,
            Some("http://new-url"),
            None,
            false,
        )
        .await;
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.name, "new-name");
        assert_eq!(updated.description.as_deref(), Some("Updated desc"));
        assert_eq!(updated.transport, "sse");
        assert_eq!(updated.url.as_deref(), Some("http://new-url"));
        assert!(!updated.is_enabled);
    }

    #[tokio::test]
    async fn test_delete_mcp_server() {
        let pool = setup().await;
        let server = create_mcp_server(
            &pool, "to-delete", None, "stdio", None, None, None, None, true,
        )
        .await;
        let deleted = delete_mcp_server(&pool, &server.id).await;
        assert!(deleted);
        let fetched = get_mcp_server(&pool, &server.id).await;
        assert!(fetched.is_none());
        // Deleting again should return false
        let deleted_again = delete_mcp_server(&pool, &server.id).await;
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_conversation_mcp_servers() {
        let pool = setup().await;
        // Create a user and conversation for the foreign key
        let user = create_user(&pool, "testuser", "test@example.com", "hash").await;
        let conv = create_conversation(&pool, &user.id, "Test Conv", None, None, None, false).await;

        let s1 = create_mcp_server(&pool, "server-a", None, "stdio", None, None, None, None, true).await;
        let s2 = create_mcp_server(&pool, "server-b", None, "sse", None, None, Some("http://b"), None, true).await;

        // Associate both servers with the conversation
        set_conversation_mcp_servers(&pool, &conv.id, &[s1.id.clone(), s2.id.clone()]).await;
        let servers = get_conversation_mcp_servers(&pool, &conv.id).await;
        assert_eq!(servers.len(), 2);

        // Replace with only one server
        set_conversation_mcp_servers(&pool, &conv.id, &[s1.id.clone()]).await;
        let servers = get_conversation_mcp_servers(&pool, &conv.id).await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "server-a");

        // Clear all associations
        set_conversation_mcp_servers(&pool, &conv.id, &[]).await;
        let servers = get_conversation_mcp_servers(&pool, &conv.id).await;
        assert_eq!(servers.len(), 0);
    }
}
