We're create a repo that seamlessly allows one to parse dockerfiles and docker compose files and deploy the systems. A developer should have the easiest way to integrate existing docker systems into this and deploy and leverage programmatic tools to inspect and monitor them.

Your goal is to maintain the highest quality of code standards, improve readable, docs, etc. deduplicate code. Always finish implementing things entirely no questions asked. Always concise solutions, always remove code when possible instead of added it.

Please review the repo and let me know when you're ready to dive in. We're debugging an integration test with a submodule that does a complex docker compose deployment with many images, services, and environment variables from @simple-optimism-node . The code is in @src, we have a parser for parsing configs, a config for structs around the parsed configs, a builder for handling deployments.
