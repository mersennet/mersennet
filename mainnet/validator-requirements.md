# Mersennet Mainnet Validator Requirements

## Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| **CPU** | 16+ cores | AMD EPYC or Intel Xeon |
| **RAM** | 64 GB | 128 GB |
| **Storage** | 2 TB NVMe SSD | 4 TB NVMe SSD |
| **Network** | 1 Gbps dedicated | 10 Gbps dedicated |

## Software Requirements

- **OS**: Ubuntu 22.04 LTS or similar (Debian 12, RHEL 9)
- **Docker**: 24.0+ (if using containerized deployment)
- **Rust**: 1.75+ (if building from source)

## Staking Requirements

- **Minimum stake**: 100,000 MRSN tokens
- **Unbonding period**: 100 blocks (configurable in genesis)

## Network Requirements

- Static public IP address
- Ports: 30303 (P2P), 8545 (RPC), 9945 (WebSocket)
- Low-latency connectivity to other validators (< 100 ms recommended)

## Security Recommendations

- Run validator in isolated network segment
- Use firewall rules to restrict RPC access
- Enable TLS for RPC endpoints in production
- Regular security updates and monitoring
