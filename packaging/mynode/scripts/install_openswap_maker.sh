#!/bin/bash

source /usr/share/mynode/mynode_device_info.sh
source /usr/share/mynode/mynode_app_versions.sh

set -x
set -e

echo "==================== INSTALLING APP ===================="

mkdir -p /opt/mynode/openswap_maker || true
mkdir -p /mnt/hdd/mynode/openswap_maker/config || true
mkdir -p /mnt/hdd/mynode/openswap_maker/openswap || true

# Remove old image if present
docker images --format '{{.Repository}}:{{.Tag}}' | grep 'openswap/maker-dashboard' | xargs --no-run-if-empty docker rmi

docker pull openswap/maker-dashboard:$VERSION
docker tag openswap/maker-dashboard:$VERSION openswap/maker-dashboard:master

echo "================== DONE INSTALLING APP ================="
