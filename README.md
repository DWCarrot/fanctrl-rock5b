# fanctrl-rock5b
A simple fan control program for rock5b; with simple linear control policy and lag-triggered state machine

## Build

build alone

```shell
cargo build

```

build for system service

```shell
## remove log timestamps and colors
cargo build --no-default-features

```


## Configuration file

Usage: `fanctrl /path/to/configuration/file`

```ini

# fanctrl v 0.2.1 configuration file 

# Path to the sensor device; like "/sys/class/thermal/thermal_zone0"
watch = /sys/class/thermal/thermal_zone0

# Path to the pwm device; like "/sys/devices/platform/fd8b0010.pwm/pwm/pwmchip1"
execute = /sys/devices/platform/fd8b0010.pwm/pwm/pwmchip1

# Interval between temperature checks, in milliseconds
interval = 5000

# Time before the pwm change when temperature drop, in times of interval
max_speed_time_cycle = 32

# Time before the pwm change when temperature drop, in times of interval
lag_time_cycle = 8

# Temperature to stop the pwm, in degrees Celsius
stop_temperature = 30.0

# Temperature to start the pwm, in degrees Celsius
start_temperature = 40.0

# Temperature when pwm should reach maximum, in degrees Celsius
high_temperature = 70.0

# Minimum duty cycle, in (0, 1)
min_duty_cycle = 0.5

# Maximum duty cycle, in (0, 1)
max_duty_cycle = 0.9

# PWM frequency, in Hz
pwm_frequency = 10000

```


## Service

1. build for system service;
2. put binary file in `/usr/local/bin/`;
3. put configuration file in `/usr/local/etc/`; 
4. put `simplefanctrl.service` service file in `/usr/local/lib/systemd/system/`;
5. enable service: `systemctl enable simplefanctrl.service`;


~~## Usage~~

```shell
Usage: fanctrl [OPTIONS] --watch <WATCH> --execute <EXECUTE>

Options:
  -i, --watch <WATCH>
          Path to the sensor device; like "/sys/class/thermal/thermal_zone0"
  -o, --execute <EXECUTE>
          Path to the pwm device; like "/sys/devices/platform/fd8b0010.pwm/pwm/pwmchip1"
  -d, --interval <INTERVAL>
          Interval between temperature checks, in milliseconds [default: 5000]
  -G, --max-speed-time-cycle <MAX_SPEED_TIME_CYCLE>
          Time before the pwm change when temperature drop, in times of interval [default: 32]
  -w, --lag-time-cycle <LAG_TIME_CYCLE>
          Time before the pwm change when temperature drop, in times of interval [default: 8]
  -0, --stop-temperature <STOP_TEMPERATURE>
          Temperature to stop the pwm, in degrees Celsius [default: 35.0]
  -1, --start-temperature <START_TEMPERATURE>
          Temperature to start the pwm, in degrees Celsius [default: 40.0]
  -2, --high-temperature <HIGH_TEMPERATURE>
          Temperature when pwm should reach maximum, in degrees Celsius [default: 70.0]
  -L, --min-duty-cycle <MIN_DUTY_CYCLE>
          Minimum duty cycle, in (0, 1) [default: 0.5]
  -U, --max-duty-cycle <MAX_DUTY_CYCLE>
          Maximum duty cycle, in (0, 1) [default: 0.9]
  -f, --pwm-frequency <PWM_FREQUENCY>
          PWM frequency, in Hz [default: 10000]
  -h, --help
          Print help
  -V, --version
          Print version
```



## Design

design.md
