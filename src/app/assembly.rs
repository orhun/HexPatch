use iced_x86::Instruction;
use ratatui::text::{Line, Span, Text};

use super::{app::App, color_settings::ColorSettings, elf::ElfHeader, pe::PEHeader};

pub enum Header
{
    Elf(ElfHeader),
    PE(PEHeader),
}

impl Header
{
    pub fn parse_header(bytes: &[u8]) -> Option<Header>
    {
        let elf_header = ElfHeader::parse_header(bytes);
        match elf_header
        {
            Some(header) => return Some(Header::Elf(header)),
            None => {},
        };
        let pe_header = PEHeader::parse_header(bytes);
        match pe_header
        {
            Some(header) => return Some(Header::PE(header)),
            None => {},
        };
        None
    }

    pub fn bitness(&self) -> u32
    {
        match self
        {
            Header::Elf(header) => header.bitness(),
            Header::PE(header) => header.bitness(),
        }
    }

    pub fn entry_point(&self) -> u64
    {
        match self
        {
            Header::Elf(header) => header.entry_point,
            Header::PE(header) => header.optional_header.address_of_entry_point as u64,
        }
    }
}

impl <'a> App<'a>
{
    fn instruction_to_line(color_settings: &ColorSettings, instruction: &Instruction, selected: bool) -> Line<'a>
    {
        let mut line = Line::default();
        line.spans.push(Span::styled(format!("{:16X}",instruction.ip()), 
            if selected
            {
                color_settings.assembly_selected
            }
            else 
            {    
                color_settings.assembly_address
            }
        ));
        line.spans.push(Span::raw(" "));
        let instruction_string = instruction.to_string();
        let mut instruction_pieces = instruction_string.split_whitespace();
        let mnemonic = instruction_pieces.next().unwrap().to_string();
        let args = instruction_pieces.collect::<Vec<&str>>().join(" ");
        let mnemonic_style = 
        match instruction.mnemonic() {
            iced_x86::Mnemonic::Nop => color_settings.assembly_nop,
            iced_x86::Mnemonic::INVALID => color_settings.assembly_bad,
            _ => color_settings.assembly_default,
        };

        line.spans.push(Span::styled(mnemonic, mnemonic_style));
        line.spans.push(Span::raw(" "));
        line.spans.push(Span::raw(args));
        line
    }

    pub(super) fn assembly_from_bytes(color_settings: &ColorSettings, bytes: &[u8]) -> (Text<'a>, Vec<usize>)
    {
        let mut output = Text::default();
        let mut line_offsets = vec![0; bytes.len()];

        let header = Header::parse_header(bytes);
        let bitness = match header
        {
            Some(header) => header.bitness(),
            None => 64,
        };

        let decoder = iced_x86::Decoder::new(bitness, bytes, iced_x86::DecoderOptions::NONE);
        let mut byte_index = 0;
        let mut line_index = 0;
        for instruction in decoder {
            
            let line = Self::instruction_to_line(color_settings, &instruction, line_index == 0);
            
            for _ in 0..instruction.len() {
                line_offsets[byte_index] = line_index;
                byte_index += 1;
            }
            line_index += 1;
            output.lines.push(line);
        }
        (output, line_offsets)
    }

    pub(super) fn update_assembly_scroll(&mut self)
    {
        let cursor_position = self.get_cursor_position();
        let current_ip = cursor_position.global_byte_index.min(self.assembly_offsets.len() - 1);
        let current_scroll = self.assembly_offsets[current_ip];
        
        self.assembly_view.lines[self.assembly_scroll].spans[0].style = self.color_settings.assembly_address;
        self.assembly_view.lines[current_scroll].spans[0].style = self.color_settings.assembly_selected;
        self.assembly_scroll = current_scroll;
    }

    pub(super) fn get_assembly_view_scroll(&self) -> usize
    {
        let visible_lines = self.screen_size.1 - 3;
        let center_of_view = visible_lines / 2;
        let view_scroll = (self.assembly_scroll as isize - center_of_view as isize).clamp(0, (self.assembly_view.lines.len() as isize - visible_lines as isize).max(0));
        
        return view_scroll as usize;
    }

    pub(super) fn edit_assembly(&mut self)
    {
        (self.assembly_view, self.assembly_offsets) = Self::assembly_from_bytes(&self.color_settings, &self.data);
        self.assembly_scroll = 0;
        self.update_assembly_scroll();
    }
}