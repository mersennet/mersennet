"""Setup script for mersennet-sdk."""

from setuptools import setup, find_packages

setup(
    name="mersennet-sdk",
    version="0.7.0",
    description="Python SDK for Mersennet - JSON-RPC, CLOB, and WebSocket client",
    long_description=open("README.md").read() if __import__("os").path.exists("README.md") else "",
    long_description_content_type="text/markdown",
    packages=find_packages(),
    python_requires=">=3.9",
    install_requires=[
        "requests>=2.28.0",
        "websocket-client>=1.5.0",
    ],
    extras_require={
        "dev": ["pytest>=7.0", "pytest-asyncio>=0.21"],
    },
)
