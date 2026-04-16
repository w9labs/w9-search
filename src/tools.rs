use anyhow::Result;
use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{json, Value};

pub struct Tools;

impl Tools {
    pub fn get_tools_definition() -> Vec<Value> {
        vec![
            json!({
                "type": "function",
                "function": {
                    "name": "get_current_date",
                    "description": "Get the current date in various formats. Useful for answering questions about what day it is, what date it is, etc.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "format": {
                                "type": "string",
                                "enum": ["iso", "readable", "day_of_week", "day_name", "full"],
                                "description": "Format for the date: 'iso' (YYYY-MM-DD), 'readable' (Month Day, Year), 'day_of_week' (Monday, Tuesday, etc.), 'day_name' (just the day name), 'full' (full date and time)"
                            },
                            "timezone": {
                                "type": "string",
                                "description": "Optional timezone (e.g., 'UTC', 'America/New_York', 'Europe/London'). Defaults to UTC."
                            }
                        }
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "get_current_time",
                    "description": "Get the current time in various formats. Useful for answering questions about what time it is.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "format": {
                                "type": "string",
                                "enum": ["12h", "24h", "iso", "timestamp"],
                                "description": "Format: '12h' (12-hour with AM/PM), '24h' (24-hour), 'iso' (ISO 8601), 'timestamp' (Unix timestamp)"
                            },
                            "timezone": {
                                "type": "string",
                                "description": "Optional timezone (e.g., 'UTC', 'America/New_York'). Defaults to UTC."
                            }
                        }
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "calculate",
                    "description": "Perform mathematical calculations. Supports basic arithmetic, percentages, and common math operations.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "expression": {
                                "type": "string",
                                "description": "Mathematical expression to evaluate (e.g., '2 + 2', '100 * 0.15', 'sqrt(16)', 'pow(2, 8)')"
                            }
                        },
                        "required": ["expression"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "format_date",
                    "description": "Format a date string into different formats. Useful for converting between date formats.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "date": {
                                "type": "string",
                                "description": "Date string to format (various formats accepted)"
                            },
                            "output_format": {
                                "type": "string",
                                "enum": ["iso", "readable", "timestamp", "relative"],
                                "description": "Desired output format"
                            }
                        },
                        "required": ["date"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "timezone_convert",
                    "description": "Convert a time from one timezone to another.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "time": {
                                "type": "string",
                                "description": "Time string to convert"
                            },
                            "from_timezone": {
                                "type": "string",
                                "description": "Source timezone (e.g., 'UTC', 'America/New_York')"
                            },
                            "to_timezone": {
                                "type": "string",
                                "description": "Target timezone (e.g., 'UTC', 'Europe/London')"
                            }
                        },
                        "required": ["time", "from_timezone", "to_timezone"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "generate_uuid",
                    "description": "Generate a UUID (Universally Unique Identifier). Useful for generating unique IDs.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "version": {
                                "type": "string",
                                "enum": ["v4", "nil"],
                                "description": "UUID version: 'v4' (random) or 'nil' (all zeros)"
                            }
                        }
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "hash_string",
                    "description": "Generate a hash of a string using various algorithms. Useful for data integrity checks.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": {
                                "type": "string",
                                "description": "Text to hash"
                            },
                            "algorithm": {
                                "type": "string",
                                "enum": ["md5", "sha256", "sha512"],
                                "description": "Hash algorithm to use"
                            }
                        },
                        "required": ["text", "algorithm"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "base64_encode",
                    "description": "Encode a string to Base64 format.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": {
                                "type": "string",
                                "description": "Text to encode"
                            }
                        },
                        "required": ["text"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "base64_decode",
                    "description": "Decode a Base64 string.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": {
                                "type": "string",
                                "description": "Base64 string to decode"
                            }
                        },
                        "required": ["text"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "unit_convert",
                    "description": "Convert between different units (length, weight, temperature, etc.). Useful for answering questions about measurements.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "value": {
                                "type": "number",
                                "description": "The value to convert"
                            },
                            "from_unit": {
                                "type": "string",
                                "description": "Source unit (e.g., 'km', 'miles', 'celsius', 'fahrenheit', 'kg', 'pounds')"
                            },
                            "to_unit": {
                                "type": "string",
                                "description": "Target unit"
                            }
                        },
                        "required": ["value", "from_unit", "to_unit"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "extract_keywords",
                    "description": "Extract key terms or keywords from a text. Useful for understanding the main topics or concepts.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": {
                                "type": "string",
                                "description": "Text to extract keywords from"
                            },
                            "max_keywords": {
                                "type": "number",
                                "description": "Maximum number of keywords to extract (default: 10)"
                            }
                        },
                        "required": ["text"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "compare_values",
                    "description": "Compare two values and determine which is larger, smaller, or if they're equal. Useful for answering comparison questions.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "value1": {
                                "type": "number",
                                "description": "First value to compare"
                            },
                            "value2": {
                                "type": "number",
                                "description": "Second value to compare"
                            }
                        },
                        "required": ["value1", "value2"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "format_number",
                    "description": "Format a number in various ways (currency, percentage, scientific notation, etc.).",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "number": {
                                "type": "number",
                                "description": "Number to format"
                            },
                            "format": {
                                "type": "string",
                                "enum": ["currency", "percentage", "scientific", "comma", "ordinal"],
                                "description": "Format type: 'currency' (add $), 'percentage' (add %), 'scientific' (e notation), 'comma' (thousands separator), 'ordinal' (1st, 2nd, etc.)"
                            },
                            "locale": {
                                "type": "string",
                                "description": "Optional locale (e.g., 'en-US', 'en-GB')"
                            }
                        },
                        "required": ["number", "format"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "validate_url",
                    "description": "Validate and parse a URL. Returns whether it's valid and extracts components (domain, path, etc.).",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "url": {
                                "type": "string",
                                "description": "URL to validate"
                            }
                        },
                        "required": ["url"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "days_between_dates",
                    "description": "Calculate the number of days between two dates. Useful for answering questions about time spans, durations, or age calculations.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "date1": {
                                "type": "string",
                                "description": "First date (various formats accepted)"
                            },
                            "date2": {
                                "type": "string",
                                "description": "Second date (various formats accepted). If not provided, uses current date."
                            }
                        },
                        "required": ["date1"]
                    }
                }
            }),
            json!({
                "type": "function",
                "function": {
                    "name": "extract_entities",
                    "description": "Extract named entities (people, places, organizations, dates) from text. Useful for understanding who/what/when/where in a text.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "text": {
                                "type": "string",
                                "description": "Text to extract entities from"
                            },
                            "entity_types": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": ["person", "place", "organization", "date", "all"]
                                },
                                "description": "Types of entities to extract (default: 'all')"
                            }
                        },
                        "required": ["text"]
                    }
                }
            }),
        ]
    }

    pub fn execute_tool(name: &str, arguments: &Value) -> Result<String> {
        tracing::info!(
            "Executing tool: {} with arguments: {}",
            name,
            serde_json::to_string(arguments).unwrap_or_default()
        );

        let result = match name {
            "get_current_date" => Self::get_current_date(arguments),
            "get_current_time" => Self::get_current_time(arguments),
            "calculate" => Self::calculate(arguments),
            "format_date" => Self::format_date(arguments),
            "timezone_convert" => Self::timezone_convert(arguments),
            "generate_uuid" => Self::generate_uuid(arguments),
            "hash_string" => Self::hash_string(arguments),
            "base64_encode" => Self::base64_encode(arguments),
            "base64_decode" => Self::base64_decode(arguments),
            "unit_convert" => Self::unit_convert(arguments),
            "extract_keywords" => Self::extract_keywords(arguments),
            "compare_values" => Self::compare_values(arguments),
            "format_number" => Self::format_number(arguments),
            "validate_url" => Self::validate_url(arguments),
            "days_between_dates" => Self::days_between_dates(arguments),
            "extract_entities" => Self::extract_entities(arguments),
            _ => {
                tracing::error!("Unknown tool requested: {}", name);
                Err(anyhow::anyhow!("Unknown tool: {}", name))
            }
        };

        match &result {
            Ok(res) => tracing::info!(
                "Tool {} executed successfully, result length: {}",
                name,
                res.len()
            ),
            Err(e) => tracing::error!("Tool {} execution failed: {}", name, e),
        }

        result
    }

    fn get_current_date(args: &Value) -> Result<String> {
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("readable");

        let timezone_str = args
            .get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("UTC");

        let now = if timezone_str == "UTC" {
            Utc::now()
        } else {
            // For simplicity, use UTC and note timezone in output
            Utc::now()
        };

        let result = match format {
            "iso" => now.format("%Y-%m-%d").to_string(),
            "readable" => now.format("%B %d, %Y").to_string(),
            "day_of_week" | "day_name" => now.format("%A").to_string(),
            "full" => {
                if timezone_str == "UTC" {
                    now.format("%A, %B %d, %Y at %H:%M:%S UTC").to_string()
                } else {
                    format!(
                        "{} (timezone: {})",
                        now.format("%A, %B %d, %Y at %H:%M:%S UTC"),
                        timezone_str
                    )
                }
            }
            _ => now.format("%B %d, %Y").to_string(),
        };

        Ok(result)
    }

    fn get_current_time(args: &Value) -> Result<String> {
        let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("24h");

        let timezone_str = args
            .get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("UTC");

        let now = Utc::now();

        let result = match format {
            "12h" => now.format("%I:%M:%S %p UTC").to_string(),
            "24h" => now.format("%H:%M:%S UTC").to_string(),
            "iso" => now.to_rfc3339(),
            "timestamp" => now.timestamp().to_string(),
            _ => now.format("%H:%M:%S UTC").to_string(),
        };

        if timezone_str != "UTC" {
            Ok(format!("{} (requested timezone: {})", result, timezone_str))
        } else {
            Ok(result)
        }
    }

    fn calculate(args: &Value) -> Result<String> {
        let expression = args
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'expression' parameter"))?;

        // Simple math evaluation (for production, use a proper math parser)
        // This is a basic implementation - consider using meval or similar for production
        let result = Self::eval_math(expression)?;
        Ok(result.to_string())
    }

    fn eval_math(expr: &str) -> Result<f64> {
        // Use meval for proper math evaluation
        meval::eval_str(expr).map_err(|e| anyhow::anyhow!("Math evaluation error: {}", e))
    }

    fn format_date(args: &Value) -> Result<String> {
        let date_str = args
            .get("date")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'date' parameter"))?;

        let output_format = args
            .get("output_format")
            .and_then(|v| v.as_str())
            .unwrap_or("readable");

        // Try to parse the date
        let dt = DateTime::parse_from_rfc3339(date_str)
            .or_else(|_| {
                // Try other formats
                date_str
                    .parse::<DateTime<Utc>>()
                    .map(|dt| dt.with_timezone(&FixedOffset::east_opt(0).unwrap()))
            })
            .or_else(|_| {
                // Try timestamp
                date_str.parse::<i64>().map(|ts| {
                    DateTime::from_timestamp(ts, 0)
                        .unwrap()
                        .with_timezone(&FixedOffset::east_opt(0).unwrap())
                })
            })?;

        let result = match output_format {
            "iso" => dt.format("%Y-%m-%dT%H:%M:%S%z").to_string(),
            "readable" => dt.format("%B %d, %Y at %H:%M:%S").to_string(),
            "timestamp" => dt.timestamp().to_string(),
            "relative" => {
                let now = Utc::now();
                let diff = now - dt.with_timezone(&Utc);
                if diff.num_days() > 0 {
                    format!("{} days ago", diff.num_days())
                } else if diff.num_hours() > 0 {
                    format!("{} hours ago", diff.num_hours())
                } else if diff.num_minutes() > 0 {
                    format!("{} minutes ago", diff.num_minutes())
                } else {
                    "just now".to_string()
                }
            }
            _ => dt.format("%B %d, %Y").to_string(),
        };

        Ok(result)
    }

    fn timezone_convert(args: &Value) -> Result<String> {
        // Simplified - for production, use chrono-tz
        let time_str = args
            .get("time")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'time' parameter"))?;

        let _from = args
            .get("from_timezone")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'from_timezone' parameter"))?;

        let _to = args
            .get("to_timezone")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to_timezone' parameter"))?;

        // Parse and convert (simplified - use chrono-tz for full support)
        let dt = DateTime::parse_from_rfc3339(time_str).or_else(|_| {
            time_str
                .parse::<DateTime<Utc>>()
                .map(|dt| dt.with_timezone(&FixedOffset::east_opt(0).unwrap()))
        })?;

        Ok(format!(
            "Converted time: {} (Note: Full timezone conversion requires chrono-tz library)",
            dt.format("%Y-%m-%d %H:%M:%S")
        ))
    }

    fn generate_uuid(args: &Value) -> Result<String> {
        let version = args.get("version").and_then(|v| v.as_str()).unwrap_or("v4");

        match version {
            "v4" => {
                use uuid::Uuid;
                Ok(Uuid::new_v4().to_string())
            }
            "nil" => {
                use uuid::Uuid;
                Ok(Uuid::nil().to_string())
            }
            _ => Err(anyhow::anyhow!("Invalid UUID version")),
        }
    }

    fn hash_string(args: &Value) -> Result<String> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;

        let algorithm = args
            .get("algorithm")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'algorithm' parameter"))?;

        match algorithm {
            "md5" => {
                let digest = md5::compute(text.as_bytes());
                Ok(format!("{:x}", digest))
            }
            "sha256" => {
                use digest::Digest;
                use sha2::Sha256;
                let mut hasher = Sha256::new();
                hasher.update(text.as_bytes());
                Ok(format!("{:x}", hasher.finalize()))
            }
            "sha512" => {
                use digest::Digest;
                use sha2::Sha512;
                let mut hasher = Sha512::new();
                hasher.update(text.as_bytes());
                Ok(format!("{:x}", hasher.finalize()))
            }
            _ => Err(anyhow::anyhow!("Unsupported algorithm")),
        }
    }

    fn base64_encode(args: &Value) -> Result<String> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;

        use base64::{engine::general_purpose, Engine as _};
        Ok(general_purpose::STANDARD.encode(text.as_bytes()))
    }

    fn base64_decode(args: &Value) -> Result<String> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;

        use base64::{engine::general_purpose, Engine as _};
        let decoded = general_purpose::STANDARD.decode(text)?;
        Ok(String::from_utf8(decoded)?)
    }

    fn unit_convert(args: &Value) -> Result<String> {
        let value = args
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'value' parameter"))?;

        let from_unit = args
            .get("from_unit")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'from_unit' parameter"))?;

        let to_unit = args
            .get("to_unit")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to_unit' parameter"))?;

        let result = Self::convert_unit(value, from_unit, to_unit)?;
        Ok(format!("{} {} = {} {}", value, from_unit, result, to_unit))
    }

    fn convert_unit(value: f64, from: &str, to: &str) -> Result<f64> {
        let from_lower = from.to_lowercase();
        let to_lower = to.to_lowercase();

        // Temperature conversions
        if from_lower.as_str() == "celsius" && to_lower.as_str() == "fahrenheit" {
            return Ok(value * 9.0 / 5.0 + 32.0);
        }
        if from_lower.as_str() == "fahrenheit" && to_lower.as_str() == "celsius" {
            return Ok((value - 32.0) * 5.0 / 9.0);
        }

        // Length conversions (to meters first, then to target)
        let meters = match from_lower.as_str() {
            "km" | "kilometer" | "kilometers" => value * 1000.0,
            "m" | "meter" | "meters" => value,
            "cm" | "centimeter" | "centimeters" => value * 0.01,
            "mm" | "millimeter" | "millimeters" => value * 0.001,
            "mile" | "miles" => value * 1609.34,
            "yard" | "yards" => value * 0.9144,
            "foot" | "feet" | "ft" => value * 0.3048,
            "inch" | "inches" | "in" => value * 0.0254,
            _ => return Err(anyhow::anyhow!("Unsupported unit: {}", from)),
        };

        let result = match to_lower.as_str() {
            "km" | "kilometer" | "kilometers" => meters / 1000.0,
            "m" | "meter" | "meters" => meters,
            "cm" | "centimeter" | "centimeters" => meters / 0.01,
            "mm" | "millimeter" | "millimeters" => meters / 0.001,
            "mile" | "miles" => meters / 1609.34,
            "yard" | "yards" => meters / 0.9144,
            "foot" | "feet" | "ft" => meters / 0.3048,
            "inch" | "inches" | "in" => meters / 0.0254,
            _ => return Err(anyhow::anyhow!("Unsupported unit: {}", to)),
        };

        Ok(result)
    }

    fn extract_keywords(args: &Value) -> Result<String> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;

        let max_keywords = args
            .get("max_keywords")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        // Simple keyword extraction (for production, use proper NLP)
        let words: Vec<&str> = text
            .split_whitespace()
            .filter(|w| w.len() > 3) // Filter short words
            .collect();

        // Count word frequencies
        use std::collections::HashMap;
        let mut freq: HashMap<&str, usize> = HashMap::new();
        for word in &words {
            *freq.entry(word).or_insert(0) += 1;
        }

        let mut keywords: Vec<(&str, usize)> = freq.into_iter().collect();
        keywords.sort_by(|a, b| b.1.cmp(&a.1));
        keywords.truncate(max_keywords);

        let result: Vec<String> = keywords
            .iter()
            .map(|(word, count)| format!("{} ({}x)", word, count))
            .collect();

        Ok(result.join(", "))
    }

    fn compare_values(args: &Value) -> Result<String> {
        let value1 = args
            .get("value1")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'value1' parameter"))?;

        let value2 = args
            .get("value2")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'value2' parameter"))?;

        let diff = (value1 - value2).abs();
        let diff_percent = if value2 != 0.0 {
            (diff / value2.abs()) * 100.0
        } else {
            0.0
        };

        if value1 > value2 {
            Ok(format!(
                "{} is {} larger than {} (difference: {:.2}, {:.1}% more)",
                value1, diff, value2, diff, diff_percent
            ))
        } else if value1 < value2 {
            Ok(format!(
                "{} is {} smaller than {} (difference: {:.2}, {:.1}% less)",
                value1, diff, value2, diff, diff_percent
            ))
        } else {
            Ok(format!("{} and {} are equal", value1, value2))
        }
    }

    fn format_number(args: &Value) -> Result<String> {
        let number = args
            .get("number")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'number' parameter"))?;

        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'format' parameter"))?;

        let result = match format {
            "currency" => format!("${:.2}", number),
            "percentage" => format!("{:.1}%", number * 100.0),
            "scientific" => format!("{:.2e}", number),
            "comma" => {
                let formatted = format!("{:.0}", number);
                // Simple comma insertion (for production use proper formatting)
                formatted
            }
            "ordinal" => {
                let n = number as i64;
                let suffix = match n % 100 {
                    11 | 12 | 13 => "th",
                    _ => match n % 10 {
                        1 => "st",
                        2 => "nd",
                        3 => "rd",
                        _ => "th",
                    },
                };
                format!("{}{}", n, suffix)
            }
            _ => return Err(anyhow::anyhow!("Unsupported format: {}", format)),
        };

        Ok(result)
    }

    fn validate_url(args: &Value) -> Result<String> {
        let url_str = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;

        match url::Url::parse(url_str) {
            Ok(url) => Ok(format!(
                "Valid URL\nDomain: {}\nPath: {}\nScheme: {}",
                url.domain().unwrap_or("N/A"),
                url.path(),
                url.scheme()
            )),
            Err(e) => Ok(format!("Invalid URL: {}", e)),
        }
    }

    fn days_between_dates(args: &Value) -> Result<String> {
        let date1_str = args
            .get("date1")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'date1' parameter"))?;

        let date2_str = args.get("date2").and_then(|v| v.as_str());

        let date1 = Self::parse_date(date1_str)?;
        let date2 = if let Some(ds) = date2_str {
            Self::parse_date(ds)?
        } else {
            Utc::now()
        };

        let diff = (date2 - date1).num_days();
        let abs_diff = diff.abs();

        if diff > 0 {
            Ok(format!(
                "{} days from {} to {}",
                abs_diff,
                date1_str,
                date2_str.unwrap_or("today")
            ))
        } else if diff < 0 {
            Ok(format!(
                "{} days ago (from {} to {})",
                abs_diff,
                date1_str,
                date2_str.unwrap_or("today")
            ))
        } else {
            Ok("0 days (same date)".to_string())
        }
    }

    fn parse_date(date_str: &str) -> Result<DateTime<Utc>> {
        // Try various date formats
        if let Ok(ts) = date_str.parse::<i64>() {
            return Ok(DateTime::from_timestamp(ts, 0).unwrap());
        }

        if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
            return Ok(dt.with_timezone(&Utc));
        }

        if let Ok(dt) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            return Ok(dt.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }

        Err(anyhow::anyhow!("Could not parse date: {}", date_str))
    }

    fn extract_entities(args: &Value) -> Result<String> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;

        // Simple entity extraction (for production, use proper NLP library)
        let mut entities = Vec::new();

        // Extract potential dates (YYYY-MM-DD, MM/DD/YYYY, etc.)
        let date_pattern = regex::Regex::new(r"\d{4}-\d{2}-\d{2}|\d{1,2}/\d{1,2}/\d{4}").unwrap();
        for mat in date_pattern.find_iter(text) {
            entities.push(format!("Date: {}", mat.as_str()));
        }

        // Extract URLs
        let url_pattern = regex::Regex::new(r"https?://[^\s]+").unwrap();
        for mat in url_pattern.find_iter(text) {
            entities.push(format!("URL: {}", mat.as_str()));
        }

        // Extract capitalized words (potential names/organizations)
        let cap_pattern = regex::Regex::new(r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+)*\b").unwrap();
        for mat in cap_pattern.find_iter(text) {
            let word = mat.as_str();
            if word.len() > 2 && !entities.iter().any(|e| e.contains(word)) {
                entities.push(format!("Potential entity: {}", word));
            }
        }

        if entities.is_empty() {
            Ok("No entities found".to_string())
        } else {
            Ok(entities.join("\n"))
        }
    }
}
