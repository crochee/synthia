---
name: api-design
description: Design REST or GraphQL endpoints with proper schemas and versioning
triggers:
  - api
  - endpoint
  - rest
  - graphql
  - restful
  - http
tags:
  - backend
  - api
  - web
levels:
  0: "name + description"
  1: "detailed instructions"
  2: "reference code snippets"
---

# API Design Skill

You are an expert in designing robust, scalable APIs. Follow RESTful principles or GraphQL conventions.

## Core Principles

1. **Consistency**: Follow consistent naming and structure
2. **Idempotency**: Safe to retry GET, PUT, DELETE operations
3. **Versioning**: Support API evolution through versioning
4. **Documentation**: Clear, comprehensive API docs

## REST API Design

### Resource Naming
- Use nouns, not verbs: `/users` not `/getUsers`
- Use plural forms: `/users` not `/user`
- Nest resources for relationships: `/users/123/orders`

### HTTP Methods
- `GET`: Retrieve resources (idempotent, safe)
- `POST`: Create new resources
- `PUT`: Update entire resources (idempotent)
- `PATCH`: Partial update
- `DELETE`: Remove resources (idempotent)

### Status Codes
- `200 OK`: Successful GET/PUT/PATCH
- `201 Created`: Successful POST
- `204 No Content`: Successful DELETE
- `400 Bad Request`: Invalid input
- `401 Unauthorized`: Missing auth
- `403 Forbidden`: Insufficient permissions
- `404 Not Found`: Resource missing
- `500 Internal Server Error`: Server failure

## Request/Response Patterns

### Standard Response
```json
{
  "data": { ... },
  "meta": {
    "page": 1,
    "perPage": 20,
    "total": 100
  }
}
```

### Error Response
```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid input",
    "details": [...]
  }
}
```

### Pagination
```
GET /resources?page=1&per_page=20&sort=created_at&order=desc
```

## Authentication

### Bearer Token
```
Authorization: Bearer <token>
```

### API Key
```
X-API-Key: <key>
```

## Versioning Strategies

### URL Path
```
/api/v1/users
/api/v2/users
```

### Header
```
Accept: application/vnd.api+json;version=2
```

## GraphQL Considerations

### Schema Design
- Use descriptive type names
- Implement proper nullability
- Use connections for lists
- Design mutations carefully

### Query Optimization
- Support field selection
- Implement dataloader for N+1
- Provide pagination cursors
