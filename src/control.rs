use std::fmt;

#[derive(Debug)]
pub struct ParameterError<T> {
    field: &'static str,
    reason: &'static str,
    value: T,
}

impl<T: fmt::Debug> fmt::Display for ParameterError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid value `{:?}` for {}: {}", self.value, self.field, self.reason)
    }
}

impl<T: fmt::Debug> std::error::Error for ParameterError<T> {
    
}




#[derive(Debug)]
pub enum ControlOutput {
    Off,
    Change(f32),
    Keep,
}


#[derive(Debug)]
pub struct Function {
    stop_temperature: f32, // T0
    start_temperature: f32, // T1
    high_temperature: f32, // T2
    min_duty_cycle: f32, // Pmin
    max_duty_cycle: f32, // Pmax
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, 
            "ReLU[T0={:.2}°C, T1={:.2}°C, T2={:.2}°C, Pmin={:.2}%, Pmax={:.2}%]", 
            self.stop_temperature, self.start_temperature, self.high_temperature, self.min_duty_cycle * 100.0, self.max_duty_cycle * 100.0)
    }
}

impl Function {

    pub fn new(stop_temperature: f32, start_temperature: f32, high_temperature: f32, min_duty_cycle: f32, max_duty_cycle: f32) -> Result<Self, ParameterError<f32>> {
        if stop_temperature >= start_temperature {
            return Err(ParameterError { field: "start_temperature", reason: "lower than stop_temperature", value: start_temperature });
        }
        if start_temperature >= high_temperature {
            return Err(ParameterError { field: "high_temperature", reason: "lower than start_temperature", value: high_temperature });
        }
        if min_duty_cycle <= 0.0 || min_duty_cycle >= 1.0 {
            return Err(ParameterError { field: "min_duty_cycle", reason: "not in (0, 1)", value: min_duty_cycle });
        }
        if max_duty_cycle <= 0.0 || max_duty_cycle >= 1.0 {
            return Err(ParameterError { field: "max_duty_cycle", reason: "not in (0, 1)", value: max_duty_cycle });
        }
        if min_duty_cycle >= max_duty_cycle {
            return Err(ParameterError { field: "max_duty_cycle", reason: "lower than min_duty_cycle", value: max_duty_cycle });
        }
        Ok(
            Self {
                stop_temperature,
                start_temperature,
                high_temperature,
                min_duty_cycle,
                max_duty_cycle,
            }
        )
    }

    pub fn map(&self, t: f32) -> f32 {
        if t < self.start_temperature {
            return self.min_duty_cycle
        }
        if t > self.high_temperature {
            return self.max_duty_cycle
        } 
        return self.min_duty_cycle + (self.max_duty_cycle - self.min_duty_cycle) * (t - self.start_temperature) / (self.high_temperature - self.start_temperature)
    }
}

#[derive(Debug)]
pub enum State {
    Off,
    Function { last_duty_cycle: f32 },
    Keep { remain_time_cycle: usize, keep_temperature: f32, keep_duty_cycle: f32 },
}


#[derive(Debug)]
pub struct Control {
    state: State,
    last_temperature: f32,
    temperature_rule: Function,
    lag_time_cycle: usize,
}

impl Control {

    pub fn new(temperature_rule: Function, lag_time_cycle: usize) -> Self {
        Self {
            state: State::Off,
            last_temperature: -273.15,
            temperature_rule,
            lag_time_cycle
        }
    }

    pub fn update(&mut self, temperature: f32) -> ControlOutput {
        let output = match &mut self.state {
            State::Off => {
                if temperature <= self.temperature_rule.start_temperature {
                    ControlOutput::Off
                } else {
                    let duty_cycle = self.temperature_rule.map(temperature);
                    self.state = State::Function { last_duty_cycle: duty_cycle };
                    ControlOutput::Change(duty_cycle)
                }
            },
            State::Function { last_duty_cycle } => {
                if temperature <= self.last_temperature {
                    self.state = State::Keep {
                        remain_time_cycle: self.lag_time_cycle,
                        keep_temperature: self.last_temperature,
                        keep_duty_cycle: *last_duty_cycle,
                    };
                    ControlOutput::Keep
                } else {
                    let duty_cycle = self.temperature_rule.map(temperature);
                    self.state = State::Function { last_duty_cycle: duty_cycle };
                    ControlOutput::Change(duty_cycle)
                }
            },
            State::Keep { remain_time_cycle, keep_temperature, keep_duty_cycle } => {
                if temperature <= self.last_temperature {
                    if *remain_time_cycle > 0 {
                        *remain_time_cycle -= 1;
                        ControlOutput::Keep
                    } else {
                        if temperature <= self.temperature_rule.stop_temperature {
                            self.state = State::Off;
                            ControlOutput::Off
                        } else {
                            *keep_temperature = (temperature + *keep_temperature) / 2.0;
                            *keep_duty_cycle = self.temperature_rule.map(*keep_temperature);
                            *remain_time_cycle = self.lag_time_cycle;
                            ControlOutput::Change(*keep_duty_cycle)
                        }
                    }
                } else {
                    let duty_cycle = self.temperature_rule.map(temperature);
                    self.state = State::Function { last_duty_cycle: duty_cycle };
                    ControlOutput::Change(duty_cycle)
                }
            },
        };
        self.last_temperature = temperature;
        output
    }

    pub fn update_force(&mut self, temperature: f32, duty_cycle: f32) -> ControlOutput {
        self.last_temperature = temperature;
        self.state = State::Keep { remain_time_cycle: self.lag_time_cycle, keep_temperature: temperature, keep_duty_cycle: duty_cycle };
        ControlOutput::Change(duty_cycle)
    }

    pub fn stop_temperature(&self) -> f32 {
        self.temperature_rule.stop_temperature
    }

    pub fn start_temperature(&self) -> f32 {
        self.temperature_rule.start_temperature
    }

    pub fn high_temperature(&self) -> f32 {
        self.temperature_rule.high_temperature
    }

    pub fn min_duty_cycle(&self) -> f32 {
        self.temperature_rule.min_duty_cycle
    }

    pub fn max_duty_cycle(&self) -> f32 {
        self.temperature_rule.max_duty_cycle
    }

    pub fn lag_time_cycle(&self) -> usize {
        self.lag_time_cycle
    }
}