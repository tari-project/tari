/// Remove diacritic marks, points and accents on lowercase characters
pub fn remove_diacritics(word: &str) -> String {
    // Replace diacritics accents
    let clean_string: String =
        word.to_lowercase()
            .as_str()
            .chars()
            .map(|x| match x {
                'a' | '\u{24D0}' | '\u{FF41}' | '\u{1E9A}' | '\u{00E0}' | '\u{00E1}' | '\u{00E2}' | '\u{1EA7}' |
                '\u{1EA5}' | '\u{1EAB}' | '\u{1EA9}' | '\u{00E3}' | '\u{0101}' | '\u{0103}' | '\u{1EB1}' |
                '\u{1EAF}' | '\u{1EB5}' | '\u{1EB3}' | '\u{0227}' | '\u{01E1}' | '\u{00E4}' | '\u{01DF}' |
                '\u{1EA3}' | '\u{00E5}' | '\u{01FB}' | '\u{01CE}' | '\u{0201}' | '\u{0203}' | '\u{1EA1}' |
                '\u{1EAD}' | '\u{1EB7}' | '\u{1E01}' | '\u{0105}' | '\u{2C65}' | '\u{0250}' => 'a',
                'b' | '\u{24D1}' | '\u{FF42}' | '\u{1E03}' | '\u{1E05}' | '\u{1E07}' | '\u{0180}' | '\u{0183}' |
                '\u{0253}' => 'b',
                'c' | '\u{24D2}' | '\u{FF43}' | '\u{0107}' | '\u{0109}' | '\u{010B}' | '\u{010D}' | '\u{00E7}' |
                '\u{1E09}' | '\u{0188}' | '\u{023C}' | '\u{A73F}' | '\u{2184}' => 'c',
                'd' | '\u{24D3}' | '\u{FF44}' | '\u{1E0B}' | '\u{010F}' | '\u{1E0D}' | '\u{1E11}' | '\u{1E13}' |
                '\u{1E0F}' | '\u{0111}' | '\u{018C}' | '\u{0256}' | '\u{0257}' | '\u{A77A}' => 'd',
                'e' | '\u{24D4}' | '\u{FF45}' | '\u{00E8}' | '\u{00E9}' | '\u{00EA}' | '\u{1EC1}' | '\u{1EBF}' |
                '\u{1EC5}' | '\u{1EC3}' | '\u{1EBD}' | '\u{0113}' | '\u{1E15}' | '\u{1E17}' | '\u{0115}' |
                '\u{0117}' | '\u{00EB}' | '\u{1EBB}' | '\u{011B}' | '\u{0205}' | '\u{0207}' | '\u{1EB9}' |
                '\u{1EC7}' | '\u{0229}' | '\u{1E1D}' | '\u{0119}' | '\u{1E19}' | '\u{1E1B}' | '\u{0247}' |
                '\u{025B}' | '\u{01DD}' => 'e',
                'f' | '\u{24D5}' | '\u{FF46}' | '\u{1E1F}' | '\u{0192}' | '\u{A77C}' => 'f',
                'g' | '\u{24D6}' | '\u{FF47}' | '\u{01F5}' | '\u{011D}' | '\u{1E21}' | '\u{011F}' | '\u{0121}' |
                '\u{01E7}' | '\u{0123}' | '\u{01E5}' | '\u{0260}' | '\u{A7A1}' | '\u{1D79}' | '\u{A77F}' => 'g',
                'h' | '\u{24D7}' | '\u{FF48}' | '\u{0125}' | '\u{1E23}' | '\u{1E27}' | '\u{021F}' | '\u{1E25}' |
                '\u{1E29}' | '\u{1E2B}' | '\u{1E96}' | '\u{0127}' | '\u{2C68}' | '\u{2C76}' | '\u{0265}' => 'h',
                'i' | '\u{24D8}' | '\u{FF49}' | '\u{00EC}' | '\u{00ED}' | '\u{00EE}' | '\u{0129}' | '\u{012B}' |
                '\u{012D}' | '\u{00EF}' | '\u{1E2F}' | '\u{1EC9}' | '\u{01D0}' | '\u{0209}' | '\u{020B}' |
                '\u{1ECB}' | '\u{012F}' | '\u{1E2D}' | '\u{0268}' | '\u{0131}' => 'i',
                'j' | '\u{24D9}' | '\u{FF4A}' | '\u{0135}' | '\u{01F0}' | '\u{0249}' => 'j',
                'k' | '\u{24DA}' | '\u{FF4B}' | '\u{1E31}' | '\u{01E9}' | '\u{1E33}' | '\u{0137}' | '\u{1E35}' |
                '\u{0199}' | '\u{2C6A}' | '\u{A741}' | '\u{A743}' | '\u{A745}' | '\u{A7A3}' => 'k',
                'l' | '\u{24DB}' | '\u{FF4C}' | '\u{0140}' | '\u{013A}' | '\u{013E}' | '\u{1E37}' | '\u{1E39}' |
                '\u{013C}' | '\u{1E3D}' | '\u{1E3B}' | '\u{017F}' | '\u{0142}' | '\u{019A}' | '\u{026B}' |
                '\u{2C61}' | '\u{A749}' | '\u{A781}' | '\u{A747}' => 'l',
                'm' | '\u{24DC}' | '\u{FF4D}' | '\u{1E3F}' | '\u{1E41}' | '\u{1E43}' | '\u{0271}' | '\u{026F}' => 'm',
                'n' | '\u{24DD}' | '\u{FF4E}' | '\u{01F9}' | '\u{0144}' | '\u{00F1}' | '\u{1E45}' | '\u{0148}' |
                '\u{1E47}' | '\u{0146}' | '\u{1E4B}' | '\u{1E49}' | '\u{019E}' | '\u{0272}' | '\u{0149}' |
                '\u{A791}' | '\u{A7A5}' => 'n',
                'o' | '\u{24DE}' | '\u{FF4F}' | '\u{00F2}' | '\u{00F3}' | '\u{00F4}' | '\u{1ED3}' | '\u{1ED1}' |
                '\u{1ED7}' | '\u{1ED5}' | '\u{00F5}' | '\u{1E4D}' | '\u{022D}' | '\u{1E4F}' | '\u{014D}' |
                '\u{1E51}' | '\u{1E53}' | '\u{014F}' | '\u{022F}' | '\u{0231}' | '\u{00F6}' | '\u{022B}' |
                '\u{1ECF}' | '\u{0151}' | '\u{01D2}' | '\u{020D}' | '\u{020F}' | '\u{01A1}' | '\u{1EDD}' |
                '\u{1EDB}' | '\u{1EE1}' | '\u{1EDF}' | '\u{1EE3}' | '\u{1ECD}' | '\u{1ED9}' | '\u{01EB}' |
                '\u{01ED}' | '\u{00F8}' | '\u{01FF}' | '\u{0254}' | '\u{A74B}' | '\u{A74D}' | '\u{0275}' => 'o',
                'p' | '\u{24DF}' | '\u{FF50}' | '\u{1E55}' | '\u{1E57}' | '\u{01A5}' | '\u{1D7D}' | '\u{A751}' |
                '\u{A753}' | '\u{A755}' => 'p',
                'q' | '\u{24E0}' | '\u{FF51}' | '\u{024B}' | '\u{A757}' | '\u{A759}' => 'q',
                'r' | '\u{24E1}' | '\u{FF52}' | '\u{0155}' | '\u{1E59}' | '\u{0159}' | '\u{0211}' | '\u{0213}' |
                '\u{1E5B}' | '\u{1E5D}' | '\u{0157}' | '\u{1E5F}' | '\u{024D}' | '\u{027D}' | '\u{A75B}' |
                '\u{A7A7}' | '\u{A783}' => 'r',
                's' | '\u{24E2}' | '\u{FF53}' | '\u{00DF}' | '\u{015B}' | '\u{1E65}' | '\u{015D}' | '\u{1E61}' |
                '\u{0161}' | '\u{1E67}' | '\u{1E63}' | '\u{1E69}' | '\u{0219}' | '\u{015F}' | '\u{023F}' |
                '\u{A7A9}' | '\u{A785}' | '\u{1E9B}' => 's',
                't' | '\u{24E3}' | '\u{FF54}' | '\u{1E6B}' | '\u{1E97}' | '\u{0165}' | '\u{1E6D}' | '\u{021B}' |
                '\u{0163}' | '\u{1E71}' | '\u{1E6F}' | '\u{0167}' | '\u{01AD}' | '\u{0288}' | '\u{2C66}' |
                '\u{A787}' => 't',
                'u' | '\u{24E4}' | '\u{FF55}' | '\u{00F9}' | '\u{00FA}' | '\u{00FB}' | '\u{0169}' | '\u{1E79}' |
                '\u{016B}' | '\u{1E7B}' | '\u{016D}' | '\u{00FC}' | '\u{01DC}' | '\u{01D8}' | '\u{01D6}' |
                '\u{01DA}' | '\u{1EE7}' | '\u{016F}' | '\u{0171}' | '\u{01D4}' | '\u{0215}' | '\u{0217}' |
                '\u{01B0}' | '\u{1EEB}' | '\u{1EE9}' | '\u{1EEF}' | '\u{1EED}' | '\u{1EF1}' | '\u{1EE5}' |
                '\u{1E73}' | '\u{0173}' | '\u{1E77}' | '\u{1E75}' | '\u{0289}' => 'u',
                'v' | '\u{24E5}' | '\u{FF56}' | '\u{1E7D}' | '\u{1E7F}' | '\u{028B}' | '\u{A75F}' | '\u{028C}' => 'v',
                'w' | '\u{24E6}' | '\u{FF57}' | '\u{1E81}' | '\u{1E83}' | '\u{0175}' | '\u{1E87}' | '\u{1E85}' |
                '\u{1E98}' | '\u{1E89}' | '\u{2C73}' => 'w',
                'x' | '\u{24E7}' | '\u{FF58}' | '\u{1E8B}' | '\u{1E8D}' => 'x',
                'y' | '\u{24E8}' | '\u{FF59}' | '\u{1EF3}' | '\u{00FD}' | '\u{0177}' | '\u{1EF9}' | '\u{0233}' |
                '\u{1E8F}' | '\u{00FF}' | '\u{1EF7}' | '\u{1E99}' | '\u{1EF5}' | '\u{01B4}' | '\u{024F}' |
                '\u{1EFF}' => 'y',
                'z' | '\u{24E9}' | '\u{FF5A}' | '\u{017A}' | '\u{1E91}' | '\u{017C}' | '\u{017E}' | '\u{1E93}' |
                '\u{1E95}' | '\u{01B6}' | '\u{0225}' | '\u{0240}' | '\u{2C6C}' | '\u{A763}' => 'z',
                _ => x,
            })
            .collect();
    // Remove any remaining non-ascii characters
    (clean_string.replace(|c: char| !c.is_ascii(), ""))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_temp() {
        // Words with Diacretics
        assert_eq!(remove_diacritics(&"ábaco".to_string()), "abaco".to_string());
        assert_eq!(remove_diacritics(&"cúpula".to_string()), "cupula".to_string());
        assert_eq!(remove_diacritics(&"legión".to_string()), "legion".to_string());
        assert_eq!(remove_diacritics(&"sureño".to_string()), "sureno".to_string());
        assert_eq!(remove_diacritics(&"chimère".to_string()), "chimere".to_string());
        assert_eq!(remove_diacritics(&"élève".to_string()), "eleve".to_string());

        // Words without Diacretics
        assert_eq!(remove_diacritics(&"observe".to_string()), "observe".to_string());
        assert_eq!(remove_diacritics(&"response".to_string()), "response".to_string());
        assert_eq!(remove_diacritics(&"bizzarro".to_string()), "bizzarro".to_string());
        assert_eq!(remove_diacritics(&"materasso".to_string()), "materasso".to_string());
    }
}
