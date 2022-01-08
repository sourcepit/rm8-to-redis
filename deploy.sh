#!/bin/bash
set -e

TARGET_HOST=pi@192.168.43.31

APP_DIR="/opt/rm8"
APP_NAME="rm8-to-redis"
APP_FILE_NAME="$APP_NAME"
APP_FILE="$APP_DIR/$APP_FILE_NAME"

CONF_DIR="/etc/rm8"
CONF_NAME=$APP_NAME
CONF_FILE_NAME="$CONF_NAME.conf"
CONF_FILE="$CONF_DIR/$CONF_FILE_NAME"

SERVICE_DIR="/etc/systemd/system"
SERVICE_NAME=$APP_NAME
SERVICE_FILE_NAME="$SERVICE_NAME.service"
SERVICE_FILE="$SERVICE_DIR/$SERVICE_FILE_NAME"

CONF="$(cat <<-EOF
	ARGS=""
EOF
)"

SERVICE="$(cat <<-EOF
	[Unit]
	Requires=network.target
	Requires=redis.service
	After=redis.service
	
	# Restart unlimitless
	StartLimitIntervalSec=0
	
	[Service]
	EnvironmentFile=$CONF_FILE
	ExecStart=$APP_FILE \\\$ARGS
	Restart=on-failure
	RestartSec=1
	
	[Install]
	WantedBy=multi-user.target
EOF
)"

scp "target/armv7-unknown-linux-gnueabihf/release/$APP_FILE_NAME" $TARGET_HOST:/tmp/$APP_FILE_NAME

ssh $TARGET_HOST -T /bin/bash << EOF
	#!/bin/bash
	set -e
	sudo su -
	
	# stop running service
	echo "stop running service"
	systemctl stop $SERVICE_NAME || true # Due to "|| true" the command always returns successful
	
	# deploy application
	mkdir -p $APP_DIR
	rm $APP_DIR/$APP_FILE_NAME
	mv /tmp/$APP_FILE_NAME $APP_DIR
	chown root:root $APP_DIR/$APP_FILE_NAME
	chmod a+x $APP_DIR/$APP_FILE_NAME
		
	# deploy configuration
	mkdir -p $CONF_DIR
	echo "$CONF_FILE"
	if [[ ! -f "$CONF_FILE" ]]; then
		echo "$CONF" > $CONF_FILE
	fi

	# deploy service
	echo "$SERVICE_FILE"
	echo "$SERVICE" > $SERVICE_FILE
	systemctl daemon-reload
	
	# start service
	echo "start service"
	systemctl start $SERVICE_NAME
	journalctl -f -u $SERVICE_NAME -o cat
EOF
	