import sys

with open('src/main.rs', 'r') as f:
    content = f.read()

# Update debug print if needed or just leave it. Let's add it to the debug struct for completeness.
content = content.replace('.field("stacks_node_rpc_url", &self.stacks_node_rpc_url)', '.field("stacks_node_rpc_url", &self.stacks_node_rpc_url)\n            .field("stacks_node_ws_url", &self.stacks_node_ws_url)')

with open('src/main.rs', 'w') as f:
    f.write(content)
