/// Утилиты, общие для ядра и голосового движка.

use chrono::Local;

/// Извлечение числа из текстовой строки (поддержка целых и дробных чисел).
///
/// Поддерживает разделители `.` и `,`.
/// Пример: `"температура 21.5"` → `Some(21.5)`
pub fn extract_number(text: &str) -> Option<f64> {
    let mut token = String::new();
    let mut found = false;

    for ch in text.chars() {
        if ch.is_ascii_digit() || (found && (ch == '.' || ch == ',')) {
            token.push(if ch == ',' { '.' } else { ch });
            found = true;
        } else if found {
            break;
        }
    }

    token.parse::<f64>().ok()
}

/// Демо-обработчик команд — общий для Tauri-команд и голосового движка.
///
/// Возвращает ответ ассистента для произнесения или отображения.
pub fn parse_demo_command(command: &str) -> String {
    let cmd = command.to_lowercase();

    // Приветствия
    if cmd.contains("привет")
        || cmd.contains("здравств")
        || cmd.contains("добрый")
    {
        return "Привет! Я Лекса, твой локальный голосовой помощник. Чем могу помочь?".to_string();
    }

    if cmd.contains("как дела") || cmd.contains("как ты") {
        return "У меня всё отлично! Я готова помогать. Что сделаем?".to_string();
    }

    if cmd.contains("кто ты") || cmd.contains("что ты") {
        return "Я Лекса — локальный голосовой помощник. Работаю на твоём устройстве. Могу управлять устройствами, озвучивать ответы и выполнять команды.".to_string();
    }

    // Устройства
    if cmd.contains("покажи устройства")
        || cmd.contains("список устройств")
        || cmd.contains("что с домом")
    {
        return "В песочнице 5 виртуальных устройств: свет в гостиной, свет в спальне, термостат, розетка рабочего стола и датчик движения.".to_string();
    }

    // Включение/выключение
    let wants_on = ["включи", "включить", "зажги", "зажечь"]
        .iter()
        .any(|word| cmd.contains(word));

    let wants_off = ["выключи", "выключить", "погаси", "погасить"]
        .iter()
        .any(|word| cmd.contains(word));

    if (wants_on || wants_off) && cmd.contains("гостин") {
        return if wants_on {
            "Включила свет в гостиной.".to_string()
        } else {
            "Выключила свет в гостиной.".to_string()
        };
    }

    if (wants_on || wants_off) && cmd.contains("спальн") {
        return if wants_on {
            "Включила свет в спальне.".to_string()
        } else {
            "Выключила свет в спальне.".to_string()
        };
    }

    // Температура
    if cmd.contains("температур") && cmd.contains("спальн") {
        if cmd.contains("какая")
            || cmd.contains("сколько")
            || cmd.contains("текущ")
        {
            return "Сейчас у виртуального термостата в спальне установлено 21 градус.".to_string();
        }

        if let Some(temp) = extract_number(command) {
            return format!(
                "Установила температуру {:.1} градуса в спальне.",
                temp
            );
        }
    }

    // Время и дата
    if cmd.contains("время") || cmd.contains("час") {
        return format!("Сейчас {}.", Local::now().format("%H:%M"));
    }

    if cmd.contains("дата") || cmd.contains("число") {
        return format!("Сегодня {}.", Local::now().format("%d.%m.%Y"));
    }

    // Прощание
    if cmd.contains("пока") || cmd.contains("до свид") {
        return "До свидания! Буду ждать тебя.".to_string();
    }

    "Я пока не знаю, как выполнить эту команду. Это демо-режим: здесь будет работать локальная языковая модель.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_integer() {
        assert_eq!(extract_number("температура 21"), Some(21.0));
    }

    #[test]
    fn extracts_decimal_with_dot() {
        assert_eq!(extract_number("температура 21.5"), Some(21.5));
    }

    #[test]
    fn extracts_decimal_with_comma() {
        assert_eq!(extract_number("температура 21,5"), Some(21.5));
    }

    #[test]
    fn returns_none_for_non_numeric() {
        assert_eq!(extract_number("нет чисел"), None);
    }

    #[test]
    fn extracts_first_number() {
        assert_eq!(extract_number("установи 22 и включи"), Some(22.0));
    }
}
