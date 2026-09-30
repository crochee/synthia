---
name: web-dev
description: Create production-grade web interfaces with high design quality
triggers:
  - website
  - web page
  - frontend
  - html
  - css
  - javascript
tags:
  - frontend
  - web
  - ui
levels:
  0: "name + description"
  1: "detailed instructions"
  2: "reference code snippets"
---

# Web Development Skill

You are an expert in creating production-grade web interfaces. Follow these principles:

## Core Principles

1. **Design Quality First**: UI should be visually distinctive and professionally crafted
2. **Responsive Design**: Works across all device sizes
3. **Accessibility**: Follow WCAG guidelines
4. **Performance**: Optimize for fast loading

## Implementation Guidelines

### HTML Structure
- Use semantic HTML5 elements
- Maintain clean, readable markup
- Include proper ARIA attributes for accessibility

### CSS Styling
- Use modern CSS features (flexbox, grid, custom properties)
- Implement mobile-first responsive design
- Use CSS variables for theming consistency
- Avoid inline styles except for dynamic values

### JavaScript
- Use vanilla JavaScript or modern frameworks as needed
- Implement proper error handling
- Optimize DOM operations
- Use modern ES6+ syntax

## Common Patterns

### Button Component
```html
<button class="btn btn-primary" type="button">
  Submit
</button>
```

### Card Layout
```html
<div class="card">
  <div class="card-header">Title</div>
  <div class="card-body">Content here</div>
</div>
```

### Form Input
```html
<div class="form-group">
  <label for="inputId">Label</label>
  <input type="text" id="inputId" class="form-control" />
</div>
```

## Best Practices

- Always validate user input
- Show loading states for async operations
- Provide clear error messages
- Use animations sparingly and purposefully
- Test across multiple browsers
