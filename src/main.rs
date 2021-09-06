// stats: https://drivehud.com/knowledgebase/what-are-the-hud-stat-definitions/
// VPIP 	Voluntarily put in pot (preflop). Any money placed into the pot voluntarily, which does not include the blinds, or folding from the big blind to a raise.
// PFR 	Pre-flop Raise. Anytime you make a raise before the flop.
// 3-Bet 	Three Bet. Anytime there’s a 3rd raise in the pot. Pre-flop, this happens on what appears to be the second raise, but in reality, the first raise counts as two raises.
// todo: win/loss, rake, AFR, Turn CB/fold, WTSD, fold to Donk, CR  etc...
// stats were minimally  checked, more testing needed out ouf every position.
// json -> binary for space efficiency
// gui
// problem if hud is closed/reopened same day, and a table is played twice same day, the table file will be processed twice
// a solution: save/reload the Files hashmap, that will reload the file offsets

#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;
use std::{thread, time};
//use std::time::{Duration, SystemTime};
//use termion::{color};
//use std::io;



const K_DIRECTORY_HISTORY_FILES: &str = "/home/nixos/.wine/drive_c/hhistory/drmalibog";
const K_REFRESH_RATE: u64 = 6; // time between two reads of history files in sec
//const K_TIME_TO_SAVE_DB_FILE: u64 = 10; // this * K_REFRESH_RATE = time between to disk save of database file
const K_TIME_TO_SAVE_DB_FILE: u64 = 1000; // this * K_REFRESH_RATE = time between to disk save of database file
//const K_TIME_TO_IGNORE_TABLE: u64 = 200; // time in sec before table is considered closed
const K_TIME_TO_IGNORE_TABLE: u64 = 20000; // time in sec before table is considered closed
const K_DATBASE_FILE: &str = "pokerhud_dbase.json";
const K_LINE_SEPARATOR: &str = "\r\n\r\n\r\n\r\n";

#[derive(Debug, Default)]
struct Action {
    name: String,
    sb: bool,
    bb: bool,
    button: bool,
    vpip: bool,
    pfr: bool,
    threeBet: bool,
    threeBetCould: bool,
    foldThreeBet: bool, // so F3B stat = number of F3B/situation where could F3B
    foldThreeBetCould: bool,
    steal: bool,
    stealCould: bool,
    foldSteal: bool,
    foldStealCould: bool,
    cbet: bool,
    cbetCould: bool,
    foldCbet: bool,
    foldCbetCould: bool,
    craise: bool,
    craiseCould: bool,
    donk: bool,
    donkCould: bool,
}

#[derive(Default, Debug)]
struct Actions(Vec<Action>);

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stat {
    handsNo: u32,
    vpip: u32,
    pfr: u32,
    threeBet: u32,
    threeBetCould: u32,
    foldThreeBet: u32,
    foldThreeBetCould: u32,
    steal: u32,
    stealCould: u32,
    foldSteal: u32,
    foldStealCould: u32,
    cbet: u32,
    cbetCould: u32,
    foldCbet: u32,
    foldCbetCould: u32,
    craise: u32,
    craiseCould: u32,
    donk: u32,
    donkCould: u32,
}

#[derive(Default, Serialize, Deserialize, Debug)]
struct Stats(HashMap<String, Stat>); // keys are player names } todo use enum instead of struct

#[derive(Default, Debug)]
struct File {
    is_active: bool, // table still open
    processed: bool, // table has its own thread already, only for multithreaded
    offset: u64,
    players: Vec<String>, // players in latest hand, ie need stats printed
}

#[derive(Default, Debug)]
struct Files(HashMap<String, File>); // key is table name

impl fmt::Display for Stat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        //            println!("{number:>width$}", number=1, width=6);

        write!(
            f,
            "{:<width$} {:<width$} {:<width$} {:<5}  {:<width$} {:<width$} {:<width$}   {:<width$} {:<width$} {:<width$} {:<width$}",
            100 * self.vpip / std::cmp::max(self.handsNo, 1),
            100 * self.pfr / std::cmp::max(self.handsNo, 1),
            100 * self.threeBet / std::cmp::max(self.threeBetCould, 1),
            self.handsNo,
            100 * self.foldThreeBet / std::cmp::max(self.foldThreeBetCould, 1),
            100 * self.steal / std::cmp::max(self.stealCould, 1),
            100 * self.foldSteal / std::cmp::max(self.foldStealCould, 1),
            100 * self.cbet / std::cmp::max(self.cbetCould, 1),
            100 * self.foldCbet / std::cmp::max(self.foldCbetCould, 1),
            100 * self.craise / std::cmp::max(self.craiseCould, 1),
            100 * self.donk / std::cmp::max(self.donkCould, 1),
            width=3
        )
    }
}

impl Actions {
    // returns a vec of players in the hand
    fn parse(&mut self, onehand: &str) -> Vec<String> {
        // expects a string containing one hand
        //
        // info to be extracted
        let mut v_button_pos: String = "".to_string();
        let mut v_button: String = "".to_string();
        let mut v_sb: String = "".to_string();
        let mut v_bb: String = "".to_string();
        let mut v_players: Vec<String> = Vec::new();
        let mut v_preflop_callers: Vec<String> = Vec::new();
        let mut v_preflop_raisers: Vec<String> = Vec::new();
        let mut v_preflop_folders: Vec<String> = Vec::new();
        let mut v_flop_callers: Vec<String> = Vec::new();
        let mut v_flop_raisers: Vec<String> = Vec::new();
        let mut v_flop_folders: Vec<String> = Vec::new();
        let mut v_flop_checkers: Vec<String> = Vec::new();
        let mut v_flop_betters: Vec<String> = Vec::new();

        // empty oneself, as the previous actions have been dealt with
        self.0.clear();
        // line iterator
        let mut lines_onehand = onehand.lines();

        // hand start ie prepreflop
        loop {
            let tline = lines_onehand.next().unwrap();

            // break if done prepreflop
            if tline.starts_with("*** HOLE CARDS ***") {
                break; // done preflope
            } // goto start of next hand

            // get button seat number
            if tline.starts_with("Table ") && tline.contains("is the button") && tline.contains("#")
            {
                let tlength = tline.len();
                v_button_pos = tline[tlength - 15..tlength - 14].to_string(); // button seat number
                continue; // next line
            } // goto start of next hand

            // populate players
            // found a player entry, add an empty action to actions
            if tline.starts_with("Seat") && tline.contains(" in chips)") && tline.contains("(") {
                // find name of button
                if tline[5..6] == v_button_pos {
                    v_button = tline[8..(tline.find('(').unwrap() - 1)].to_string();
                }

                // recover name and push empty entry to actions
                let name = tline[8..(tline.find('(').unwrap() - 1)].to_string(); // problem if the name contains '(' then the rest of the name will not be parsed
                                                                                 // push player to vec of players
                v_players.push(name.clone());

                // push player's action
                self.0.push(Action {
                    name,
                    sb: false,
                    bb: false,
                    button: false,
                    vpip: false,
                    pfr: false,
                    threeBet: false,
                    threeBetCould: false,
                    foldThreeBet: false,
                    foldThreeBetCould: false,
                    steal: false,
                    stealCould: false,
                    foldSteal: false,
                    foldStealCould: false,
                    cbet: false,
                    cbetCould: false,
                    foldCbet: false,
                    foldCbetCould: false,
                    craise: false,
                    craiseCould: false,
                    donk: false,
                    donkCould: false,
                });

                continue; // next line
            }
            // sb line
            if tline.contains(": posts small blind") {
                v_sb = tline[0..tline.find(':').unwrap()].to_string();
                continue; // next line
            }
            if tline.contains(": posts big blind") {
                v_bb = tline[0..tline.find(':').unwrap()].to_string();
                continue; // next line
            }
        }

        // preflop and flop
        'outer: loop {
            //read summary ie there's no flop. if there's a flop it will be taken care inside this loop
            let tline = lines_onehand.next().unwrap();
            if tline.starts_with("*** SUMMARY ***") {
                break;
            }

            // preflop
            if tline.contains(": folds") {
                // problem if player name contains ": folds $"
                let name = tline[0..tline.find(':').unwrap()].to_string();
                v_preflop_folders.push(name);
                continue; // next line
            }

            if tline.contains(": calls $") {
                // problem if player name contains "calls"
                let name = tline[0..tline.find(':').unwrap()].to_string();
                v_preflop_callers.push(name);
                continue; // next line
            }

            if tline.contains(": bets $") {
                // don't think PS uses "bets" preflop, but just in case.
                let name = tline[0..tline.find(':').unwrap()].to_string();
                v_preflop_raisers.push(name);
                continue; // next line
            }

            if tline.contains(": raises $") {
                let name = tline[0..tline.find(':').unwrap()].to_string();
                v_preflop_raisers.push(name);
                continue; // next line
            }

            // flop
            if tline.starts_with("*** FLOP ***") {
                loop {
                    // until reach turn or end of hand in case no turn
                    let tline = lines_onehand.next().unwrap();
                    if tline.starts_with("*** SUMMARY ***") || tline.starts_with("*** TURN ***") {
                        // end of hand
                        break 'outer; // done with the hand
                    }
                    if tline.contains(": folds") {
                        // problem if player name contains ": folds"
                        let name = tline[0..tline.find(':').unwrap()].to_string();
                        v_flop_folders.push(name);
                        continue; // next flop line
                    }

                    if tline.contains(": calls $") {
                        // problem if player name contains "calls"
                        let name = tline[0..tline.find(':').unwrap()].to_string();
                        v_flop_callers.push(name);
                        continue; // next flop line
                    }

                    if tline.contains(": bets $") {
                        let name = tline[0..tline.find(':').unwrap()].to_string();
                        v_flop_betters.push(name);
                        continue; // next flop line
                    }

                    if tline.contains(": raises $") {
                        let name = tline[0..tline.find(':').unwrap()].to_string();
                        v_flop_raisers.push(name);
                        continue; // next flop line
                    }
                    if tline.contains(": checks") {
                        let name = tline[0..tline.find(':').unwrap()].to_string();
                        v_flop_checkers.push(name);
                        continue; // next flop line
                    }
                }
            }
        }

        // function to find first pos of string in a vec // returns length if not found
        fn pos_no(name: &str, vec: &Vec<String>) -> i32 {
            let mut c: i32 = 0;
            for names in vec {
                if name == names {
                    return c;
                }
                c += 1;
            }
            c
        }

        fn is_in(name: &str, vec: &Vec<String>) -> bool {
            for names in vec {
                if name == names {
                    return true;
                }
            }
            false
        }
        // a mod b
        fn modulo(a: i32, b: i32) -> i32 {
            ((a % b) + b) % b
        }

        // fill up actions
        for action in &mut self.0 {
            action.sb = action.name == v_sb;
            action.bb = action.name == v_bb;
            action.button = action.name == v_button;

            action.vpip =
                is_in(&action.name, &v_preflop_callers) || is_in(&action.name, &v_preflop_raisers);

            action.pfr = is_in(&action.name, &v_preflop_raisers);

            action.threeBet = v_preflop_raisers.len() > 1 && v_preflop_raisers[1] == action.name;

            let pos_utg = modulo(pos_no(&v_bb, &v_players) + 1, v_players.len() as i32);
            action.threeBetCould = v_preflop_raisers.len() as i32 > 0 // there's a pfr
                && ( modulo( pos_no(&v_preflop_raisers[0], &v_players) - pos_utg , v_players.len() as i32 ) < modulo( pos_no(&action.name, &v_players) - pos_utg , v_players.len() as i32 ) // player acts after pfr  
                    || (modulo( pos_no(&v_preflop_raisers[0], &v_players) - pos_utg , v_players.len() as i32 ) > modulo( pos_no(&action.name, &v_players) - pos_utg , v_players.len() as i32) && is_in(&action.name, &v_preflop_callers)));

            action.foldThreeBet = v_preflop_raisers.len() == 2
                && v_preflop_raisers[0] == action.name
                && is_in(&action.name, &v_preflop_folders);
            action.foldThreeBetCould =
                v_preflop_raisers.len() == 2 && v_preflop_raisers[0] == action.name;

            action.steal = action.name == v_button
                && v_preflop_raisers.len() > 0
                && v_preflop_raisers[0] == action.name;

            action.stealCould = action.name == v_button
                && (v_preflop_raisers.len() == 0
                    || v_preflop_raisers[0] == v_sb
                    || v_preflop_raisers[0] == v_bb
                    || v_preflop_raisers[0] == v_button);

            action.foldSteal = if action.name == v_sb
                && v_preflop_raisers.len() > 0
                && v_preflop_raisers[0] == v_button
                && is_in(&action.name, &v_preflop_raisers) == false
                && is_in(&action.name, &v_preflop_callers) == false
                && is_in(&action.name, &v_preflop_folders)
            {
                true
            }
            // sb folds to btn bet
            else if action.name == v_bb
                && v_preflop_raisers.len() == 1
                && v_preflop_raisers[0] == v_button
                && is_in(&action.name, &v_preflop_raisers) == false
                && is_in(&action.name, &v_preflop_callers) == false
                && is_in(&action.name, &v_preflop_folders)
            {
                true
            }
            // bb folds to btn bet
            else {
                false
            };

            action.foldStealCould = if action.name == v_sb
                && v_preflop_raisers.len() > 0
                && v_preflop_raisers[0] == v_button
            {
                true
            }
            // sb folds to btn bet
            else if action.name == v_bb
                && v_preflop_raisers.len() == 1
                && v_preflop_raisers[0] == v_button
            {
                true
            }
            // bb folds to btn bet
            else {
                false
            };

            action.cbet = v_preflop_raisers.len() == 1
                && action.name == v_preflop_raisers[0]
                && v_flop_betters.len() == 1
                && action.name == v_flop_betters[0];
            action.cbetCould = v_preflop_raisers.len() == 1 && action.name == v_preflop_raisers[0]; // we'd need to check there's a flop and all check to him...

            // folds to flop bet in single raised pot
            action.foldCbet = v_preflop_raisers.len() == 1 // one raise preflop 
                && is_in(&action.name, &v_preflop_callers) // player calls pre
                && v_flop_betters.len() == 1 // 1 bet flop
                && v_flop_betters[0] == v_preflop_raisers[0] // pfr bets flop
                && v_flop_raisers.len() == 0 // no flop raise
                && is_in(&action.name, &v_flop_folders); // player folds flop

            action.foldCbetCould = v_preflop_raisers.len() == 1 // one raise preflop 
                && is_in(&action.name, &v_preflop_callers) // player calls pre
                && v_flop_betters.len() == 1 // 1 bet flop
                && v_flop_betters[0] == v_preflop_raisers[0]; // pfr bets flop

            action.craise =
                is_in(&action.name, &v_flop_checkers) && is_in(&action.name, &v_flop_raisers);

            action.craiseCould = is_in(&action.name, &v_flop_checkers) && v_flop_betters.len() > 0;

            let pos_sb = pos_no(&v_bb, &v_players);
            action.donk = v_preflop_raisers.len() == 1 // one better pre
                && is_in(&action.name, &v_preflop_folders) == false // he didn't fold pre
            && action.name != v_preflop_raisers[0] // he aint the pfr
            && modulo( pos_no(&v_preflop_raisers[0], &v_players) - pos_sb , v_players.len() as i32 ) > modulo( pos_no(&action.name, &v_players) - pos_sb , v_players.len()  as i32) // player acts before pfr  
                && is_in(&action.name, &v_flop_betters); // he's first better, we dont check there isnt a donk already

            action.donkCould = v_preflop_raisers.len() == 1 // one better pre
                && is_in(&action.name, &v_preflop_folders) == false // he didn't fold pre
            && action.name != v_preflop_raisers[0] // he aint the pfr
            && modulo(
                pos_no(&v_preflop_raisers[0], &v_players) - pos_sb,
                v_players.len() as i32,
            ) > modulo(pos_no(&action.name, &v_players) - pos_sb, v_players.len() as i32)
            // player acts before pfr
        }
        return v_players;
    }
}

impl Stats {
    fn populate(&mut self, actions: &Actions) {
        for action in &actions.0 {
            //           let player = action.name.clone();
            let elem = self
                .0
                .entry(action.name.clone())
                .or_insert(<Stat as Default>::default()); // if entry does not exist, push new empty stat

            (*elem).handsNo += 1; // todo could write a map or a macro here?
            (*elem).vpip += if action.vpip { 1 } else { 0 };
            (*elem).pfr += if action.pfr { 1 } else { 0 };
            (*elem).threeBet += if action.threeBet { 1 } else { 0 };
            (*elem).threeBetCould += if action.threeBetCould { 1 } else { 0 };
            (*elem).foldThreeBet += if action.foldThreeBet { 1 } else { 0 };
            (*elem).foldThreeBetCould += if action.foldThreeBetCould { 1 } else { 0 };
            (*elem).steal += if action.steal { 1 } else { 0 };
            (*elem).stealCould += if action.stealCould { 1 } else { 0 };
            (*elem).foldSteal += if action.foldSteal { 1 } else { 0 };
            (*elem).foldStealCould += if action.foldStealCould { 1 } else { 0 };
            (*elem).cbet += if action.cbet { 1 } else { 0 };
            (*elem).cbetCould += if action.cbetCould { 1 } else { 0 };
            (*elem).foldCbet += if action.foldCbet { 1 } else { 0 };
            (*elem).foldCbetCould += if action.foldCbetCould { 1 } else { 0 };
            (*elem).craise += if action.craise { 1 } else { 0 };
            (*elem).craiseCould += if action.craiseCould { 1 } else { 0 };
            (*elem).donk += if action.donk { 1 } else { 0 };
            (*elem).donkCould += if action.donkCould { 1 } else { 0 };
        }
    }

    fn update(&mut self, files: &mut Files) {
        // set all tables to not active
        for (_, file) in files.0.iter_mut() {
            (*file).is_active = false;
        }

        if let Ok(entries) = fs::read_dir(K_DIRECTORY_HISTORY_FILES) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let mut contents = String::new();
                    if let Ok(metadata) = fs::metadata(&entry.path()) {
                        if metadata
                            .modified()
                            .unwrap()
                            .elapsed()
                            .unwrap_or_default()
                            .as_secs()
                            < K_TIME_TO_IGNORE_TABLE
                        // table is still played on
                        {
                            // insert entry if not there yet
                            let table_name = entry.path().to_str().unwrap().to_string();
                            let elem = files
                                .0
                                .entry(table_name.clone())
                                .or_insert(<File as Default>::default()); // if entry does not exist, push new empty file entry

                            // set table to alive
                            (*elem).is_active = true; // set table to active

                            // cut hand history into strings of hands
                            if let Ok(mut file) = fs::File::open(&entry.path()) {
                                if let Ok(off) = file.seek(SeekFrom::End(0)) {
                                    if off > (*elem).offset {
                                        // goto previous offset
                                        if let Ok(_) = file.seek(SeekFrom::Start((*elem).offset)) {
                                            if let Ok(_) = file.read_to_string(&mut contents) {
                                                let hands: Vec<&str> = contents
                                                    .split(K_LINE_SEPARATOR)
                                                    .filter(|s| !s.is_empty())
                                                    .collect();

                                                // update offset
                                                (*elem).offset = off;

                                                let mut players: Vec<String> = Vec::new();
                                                // process hands
                                                for hand in &hands {
                                                    let mut actions: Actions = Default::default();
                                                    players = actions.parse(&hand);
                                                    self.populate(&actions);
                                                }

                                                // update active players
                                                if players.len() > 0 {
                                                    (*elem).players = players;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn print(&self, files: &Files) {
        // clear screen
        print!("{}[2J", 27 as char); //clear screen
        print!("{esc}[2J{esc}[1;1H", esc = 27 as char); // put cursor top
        let mut players: Vec<&str> = Vec::new(); // active players
        for (_, file) in &files.0 {
            if file.is_active {
                for player in &file.players {
                    players.push(player);
                }
            }
        }
        players.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        players.dedup(); // order and remove duplicates

        println!("{:<14} {:<width$} {:<width$} {:<width$} {:<5}  {:<width$} {:<width$} {:<width$}   {:<width$} {:<width$} {:<width$} {:<width$}",
                 "Player:", "vpi", "pfr", "3B", "No", "F3B", "ST", "FS", "CB", "FCB", "CR", "Dk", width=3);
        for player in players {
            println!("{:<14} {}", player, self.0.get(player).unwrap());
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut stats: Stats = Default::default();
    let mut files: Files = Default::default();
    let mut counter = 0;

    // recover dbase from disk
    if Path::new(K_DATBASE_FILE).exists() {
        let mut file = fs::File::open(K_DATBASE_FILE)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        stats = serde_json::from_str(&contents).unwrap();
    }

    loop {
        counter += 1;
        // get latest handhistories
        stats.update(&mut files);

        // save db to disk
        if counter % K_TIME_TO_SAVE_DB_FILE == 0 {
            let serialized = serde_json::to_string(&stats).unwrap();
            fs::write(K_DATBASE_FILE, &serialized.into_bytes())?;
        }

        stats.print(&files);
        //
        // sleep
        let delay = time::Duration::from_secs(K_REFRESH_RATE);
        thread::sleep(delay);
    }
    //    Ok(())
}
