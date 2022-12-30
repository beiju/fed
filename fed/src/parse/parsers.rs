use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, take_till, take_till1, take_until1};
use nom::{AsChar, IResult, Parser};
use nom::character::complete::{char, digit1};
use nom::combinator::{fail, map_res, opt, recognize, verify};
use nom::multi::{many0, separated_list1};
use nom::number::complete::float;
use nom::sequence::{pair, preceded, terminated};

use crate::{EchoChamberModAdded, TimeElsewhere};
use crate::parse::event_schema::{ActivePositionType, AttrCategory, ModDuration};

pub(crate) type ParserError<'a> = nom::error::VerboseError<&'a str>;
pub(crate) type ParserResult<'a, Out> = IResult<&'a str, Out, ParserError<'a>>;

pub(crate) fn parse_terminated(tag_content: &str) -> impl Fn(&str) -> ParserResult<&str> + '_ {
    move |input| {
        let (input, parsed_value) = if tag_content == "." {
            alt((
                // The Kaj Statter Jr. rule
                verify(recognize(terminated(take_until1(".."), tag("."))), |s: &str| !s.contains('\n')),
                verify(take_until1(tag_content), |s: &str| !s.contains('\n')),
            )).parse(input)
        } else {
            verify(take_until1(tag_content), |s: &str| !s.contains('\n')).parse(input)
        }?;
        let (input, _) = tag(tag_content).parse(input)?;

        Ok((input, parsed_value))
    }
}

// This is for use in place of parse_terminated when the only remaining text in the string is ".",
// and so you can't use parse_terminated because that would improperly cut off names with periods
// like "Kaj Statter Jr."
pub(crate) fn parse_until_period_eof(input: &str) -> ParserResult<&str> {
    let (input, replacement_name_with_dot) = is_not("\n").parse(input)?;
    let replacement_name = replacement_name_with_dot.strip_suffix(".")
        .ok_or_else(|| {
            // I can't figure out how to make an error myself so I'm just gonna unwrap a fail
            fail::<_, (), _>(replacement_name_with_dot).unwrap_err()
        })?;

    Ok((input, replacement_name))
}

pub(crate) fn parse_half_inning(input: &str) -> ParserResult<(bool, i32, &str)> {
    let (input, top_of_inning) = alt((
        tag("Top").map(|_| true),
        tag("Bottom").map(|_| false),
    )).parse(input)?;

    let (input, _) = tag(" of ").parse(input)?;
    let (input, inning) = parse_whole_number(input)?;

    let (input, _) = tag(", ").parse(input)?;
    let (input, team_name) = parse_terminated(" batting.").parse(input)?;

    Ok((input, (top_of_inning, inning, team_name)))
}

pub(crate) fn parse_whole_number(input: &str) -> ParserResult<i32> {
    map_res(digit1, str::parse).parse(input)
}

pub(crate) fn parse_batter_up(input: &str) -> ParserResult<(&str, Option<&str>, &str, Option<&str>, bool)> {
    let (input, repeating) = opt(parse_terminated("is Repeating!\n")).parse(input)?;
    let (input, (batter_name, inhabiting_name)) = alt((
        // NOTE order matters here. inhabiting must be first
        parse_batter_up_inhabiting,
        parse_terminated(" batting for the ").map(|n| (n, None)),
    )).parse(input)?;
    // This is going to fail if a team ever has a period or comma in it
    let (input, team_name) = take_till1(|c| c == ',' || c == '.').parse(input)?;
    let (input, wielding_item) = alt((
        // No legacy item
        tag(".").map(|_| None),
        // Legacy item
        parse_wielding_item.map(|s| Some(s))
    )).parse(input)?;

    Ok((input, (batter_name, inhabiting_name, team_name, wielding_item, repeating.is_some())))
}

pub(crate) fn parse_batter_up_inhabiting(input: &str) -> ParserResult<(&str, Option<&str>)> {
    let (input, batter_name) = parse_terminated(" is Inhabiting ").parse(input)?;
    let (input, inhabiting_name) = parse_terminated("!\n").parse(input)?;
    let (input, _) = tag(batter_name).parse(input)?;
    let (input, _) = tag(" batting for the ").parse(input)?;

    Ok((input, (batter_name, Some(inhabiting_name))))
}

pub(crate) fn parse_wielding_item(input: &str) -> ParserResult<&str> {
    let (input, _) = tag(", wielding ").parse(input)?;
    // can't use parse_terminated because the terminator would be "." and "the Iffey Jr." exists
    if let Some((idx, end)) = input.rmatch_indices('.').next() {
        let (input, item_name) = (end, &input[0..idx]);
        let (input, _) = tag(".").parse(input)?;
        Ok((input, item_name))
    } else {
        fail(input)
    }
}

pub(crate) fn parse_ball(input: &str) -> ParserResult<(i32, i32)> {
    let (input, _) = tag("Ball. ").parse(input)?;
    let (input, count) = parse_count(input)?;

    Ok((input, count))
}

pub(crate) fn parse_foul_ball(input: &str) -> ParserResult<(i32, i32)> {
    let (input, _) = tag("Foul Ball. ").parse(input)?;
    let (input, count) = parse_count(input)?;

    Ok((input, count))
}

pub enum StrikeType {
    Swinging,
    Looking,
    Flinching,
}

pub(crate) fn parse_strike(input: &str) -> ParserResult<(StrikeType, i32, i32)> {
    let (input, _) = tag("Strike, ").parse(input)?;
    let (input, strike_type) = alt((
        tag("swinging. ").map(|_| StrikeType::Swinging),
        tag("looking. ").map(|_| StrikeType::Looking),
        tag("flinching. ").map(|_| StrikeType::Flinching),
    )).parse(input)?;
    let (input, (balls, strikes)) = parse_count(input)?;

    Ok((input, (strike_type, balls, strikes)))
}

pub(crate) fn parse_count(input: &str) -> ParserResult<(i32, i32)> {
    // this should handle double-digit counts because i know how blaseball is
    let (input, balls) = parse_whole_number(input)?;
    let (input, _) = tag("-").parse(input)?;
    let (input, strikes) = parse_whole_number(input)?;

    Ok((input, (balls, strikes)))
}

pub(crate) fn parse_flyout(input: &str) -> ParserResult<(&str, &str)> {
    let (input, batter_name) = parse_terminated(" hit a flyout to ").parse(input)?;
    let (input, fielder_name) = parse_terminated(".").parse(input)?;

    Ok((input, (batter_name, fielder_name)))
}

pub(crate) fn parse_batter_debt<'a>(batter_name: &'a str, fielder_name: &'a str) -> impl Fn(&str) -> ParserResult<()> + 'a {
    move |input: &str| {
        let (input, _) = tag("\n").parse(input)?;
        let (input, _) = tag(batter_name).parse(input)?;
        let (input, _) = tag(" hit a ball at ").parse(input)?;
        let (input, _) = tag(fielder_name).parse(input)?;
        let (input, _) = tag("...\n").parse(input)?;
        let (input, _) = tag(fielder_name).parse(input)?;
        let (input, _) = tag(" is now being Observed.").parse(input)?;

        Ok((input, ()))
    }
}

pub(crate) enum ParsedGroundOut<'a> {
    Simple {
        batter_name: &'a str,
        fielder_name: &'a str,
    },
    FieldersChoice {
        runner_out_name: &'a str,
        base: i32,
    },
    DoublePlay {
        batter_name: &'a str,
    },
}

pub(crate) fn parse_ground_out(input: &str) -> ParserResult<ParsedGroundOut> {
    alt((parse_simple_ground_out, parse_fielders_choice, parse_double_play)).parse(input)
}

pub(crate) fn parse_simple_ground_out(input: &str) -> ParserResult<ParsedGroundOut> {
    let (input, batter_name) = parse_terminated(" hit a ground out to ").parse(input)?;
    let (input, fielder_name) = parse_terminated(".").parse(input)?;

    let parsed = ParsedGroundOut::Simple {
        batter_name,
        fielder_name,
    };
    Ok((input, (parsed)))
}

pub(crate) fn parse_fielders_choice(input: &str) -> ParserResult<ParsedGroundOut> {
    let (input, runner_out_name) = parse_terminated(" out at ").parse(input)?;
    let (input, base) = parse_named_base(input)?;
    let (input, _) = tag(" base.").parse(input)?;

    Ok((input, (ParsedGroundOut::FieldersChoice { runner_out_name, base })))
}

pub(crate) fn parse_reaches_on_fielders_choice(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("\n").parse(input)?;
    let (input, batter_name) = parse_terminated(" reaches on fielder's choice.").parse(input)?;

    Ok((input, batter_name))
}

pub(crate) fn parse_double_play(input: &str) -> ParserResult<ParsedGroundOut> {
    let (input, batter_name) = parse_terminated(" hit into a double play!").parse(input)?;

    Ok((input, (ParsedGroundOut::DoublePlay { batter_name })))
}

pub(crate) fn parse_hit(input: &str) -> ParserResult<(&str, i32, Option<&str>)> {
    let (input, broke) = opt(parse_item_damage_unknown_name(false, false)).parse(input)?;
    let (input, batter_name, broken_item_name) = if let Some((broken_item_name, batter_name)) = broke {
        let (input, _) = tag("\n").parse(input)?;
        let (input, _) = tag(batter_name).parse(input)?;
        let (input, _) = tag(" hits a ").parse(input)?;

        (input, batter_name, Some(broken_item_name))
    } else {
        let (input, batter_name) = parse_terminated(" hits a ").parse(input)?;

        (input, batter_name, None)
    };
    let (input, num_bases) = alt((
        tag("Single!").map(|_| 1),
        tag("Double!").map(|_| 2),
        tag("Triple!").map(|_| 3),
        tag("Quadruple!").map(|_| 4),
    )).parse(input)?;

    Ok((input, (batter_name, num_bases, broken_item_name)))
}

#[derive(PartialEq)]
pub(crate) enum ParsedSpicyStatus {
    None,
    HeatingUp,
    RedHot,
}

pub(crate) fn parse_spicy_status(batter_name: &str) -> impl FnMut(&str) -> ParserResult<ParsedSpicyStatus> + '_ {
    move |input: &str| {
        let (input, heating_up) = opt(alt((
            terminated(terminated(char('\n'), tag(batter_name)), tag(" is Heating Up!")).map(|_| ParsedSpicyStatus::HeatingUp),
            terminated(terminated(char('\n'), tag(batter_name)), tag(" is Red Hot!")).map(|_| ParsedSpicyStatus::RedHot),
        ))).parse(input)?;
        Ok((input, heating_up.unwrap_or(ParsedSpicyStatus::None)))
    }
}

pub(crate) fn parse_cooled_off(batter_name: &str) -> impl FnMut(&str) -> ParserResult<bool> + '_ {
    move |input: &str| {
        let (input, cooled_off) = opt(
            terminated(terminated(char('\n'), tag(batter_name)), tag(" cooled off.")),
        ).parse(input)?;
        Ok((input, cooled_off.is_some()))
    }
}

pub(crate) fn parse_free_refill(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("\n").parse(input)?;
    let (input, name) = parse_terminated(" used their Free Refill.\n").parse(input)?;
    let (input, _) = tag(name).parse(input)?;
    let (input, _) = tag(" Refills the In!").parse(input)?;

    Ok((input, name))
}

type ParsedScore<'a> = (Option<&'a str>, &'a str);

pub(crate) fn parse_scores<'a>(score_label: &'static str, extra_space: bool) -> impl FnMut(&'a str) -> ParserResult<Vec<ParsedScore<'a>>> {
    move |input| {
        let (input, scorers) = many0(parse_score(score_label, extra_space)).parse(input)?;

        Ok((input, scorers))
    }
}

pub(crate) fn parse_score(score_label: &'static str, extra_space: bool) -> impl Fn(&str) -> ParserResult<ParsedScore> {
    move |input| {
        let (input, item) = opt(parse_item_damage_unknown_name(extra_space, true)).parse(input)?;
        let (input, _) = tag("\n").parse(input)?;
        if let Some((item_name, player_name)) = item {
            let (input, _) = tag(player_name).parse(input)?;
            let (input, _) = tag(score_label).parse(input)?;

            Ok((input, (Some(item_name), player_name)))
        } else {
            let (input, name) = parse_terminated(score_label).parse(input)?;
            Ok((input, (None, name)))
        }
    }
}

pub(crate) fn parse_magmatic(input: &str) -> ParserResult<Option<&str>> {
    opt(parse_terminated(" is Magmatic!\n")).parse(input)
}

pub(crate) fn parse_hr(input: &str) -> ParserResult<(&str, i32)> {
    let (input, batter_name) = parse_terminated(" hits a ").parse(input)?;
    let (input, num_runs) = alt((
        tag("solo home run!").map(|_| 1),
        tag("2-run home run!").map(|_| 2),
        tag("3-run home run!").map(|_| 3),
        tag("grand slam!").map(|_| 4), // dunno what happens with a pentaslam...
    )).parse(input)?;

    Ok((input, (batter_name, num_runs)))
}

pub(crate) fn parse_attract_player(input: &str) -> ParserResult<Option<(&str, &str)>> {
    opt(parse_attract_player_inner).parse(input)
}

pub(crate) fn parse_attract_player_inner(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("\nThe ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" Attract ").parse(input)?;
    let (input, player_name) = parse_terminated("!").parse(input)?;

    Ok((input, (team_nickname, player_name)))
}

pub(crate) fn parse_big_bucket(input: &str) -> ParserResult<bool> {
    let (input, big_buckets) = opt(tag("\nThe ball lands in a Big Bucket. An extra Run scores!")).parse(input)?;
    Ok((input, big_buckets.is_some()))
}

pub(crate) fn parse_free_refills(input: &str) -> ParserResult<Vec<&str>> {
    many0(parse_free_refill).parse(input)
}

pub(crate) fn parse_stolen_base(input: &str) -> ParserResult<(&str, i32, bool, bool, Option<&str>)> {
    let (input, (runner_name, is_successful)) = alt((
        parse_terminated(" steals ").map(|n| (n, true)),
        parse_terminated(" gets caught stealing ").map(|n| (n, false)),
    )).parse(input)?;

    let (input, num_runs) = parse_named_base(input)?;

    // Decide whether to be excited
    let (input, _) = tag(if is_successful { " base!" } else { " base." }).parse(input)?;

    let (input, blaserunning) = opt(preceded(tag("\n"), preceded(tag(runner_name), tag(" scores with Blaserunning!")))).parse(input)?;
    let (input, free_refill) = opt(parse_free_refill).parse(input)?;

    Ok((input, (runner_name, num_runs, is_successful, blaserunning.is_some(), free_refill)))
}

pub(crate) fn parse_named_base(input: &str) -> ParserResult<i32> {
    alt((
        tag("first").map(|_| 1),
        tag("second").map(|_| 2),
        tag("third").map(|_| 3),
        tag("fourth").map(|_| 4),
        tag("fifth").map(|_| 5),
    )).parse(input)
}

pub(crate) enum ParsedStrikeout<'a> {
    Swinging(&'a str),
    Looking(&'a str),

    Charm {
        charmer_name: &'a str,
        charmed_name: &'a str,
        num_swings: i32,
    },
}

pub(crate) fn parse_strikeout(input: &str) -> ParserResult<ParsedStrikeout> {
    alt((
        parse_normal_strikeout,
        parse_charm_strikeout
    )).parse(input)
}

pub(crate) fn parse_normal_strikeout(input: &str) -> ParserResult<ParsedStrikeout> {
    let (input, batter_name) = parse_terminated(" strikes out ").parse(input)?;
    let (input, is_swinging) = alt((
        tag("swinging.").map(|_| true),
        tag("looking.").map(|_| false),
    )).parse(input)?;

    Ok((input, if is_swinging { ParsedStrikeout::Swinging(batter_name) } else { ParsedStrikeout::Looking(batter_name) }))
}

pub(crate) fn parse_charm_strikeout(input: &str) -> ParserResult<ParsedStrikeout> {
    let (input, charmer_name) = parse_terminated(" charmed ").parse(input)?;
    let (input, charmed_name) = parse_terminated("!\n").parse(input)?;
    let (input, charmed_name2) = parse_terminated(" swings ").parse(input)?;
    let (input, num_swings) = parse_whole_number(input)?;
    let (input, _) = tag(" times to strike out willingly!").parse(input)?;

    // I believe these should always be equal
    assert_eq!(charmed_name, charmed_name2);

    Ok((input, ParsedStrikeout::Charm { charmer_name, charmed_name, num_swings }))
}

pub(crate) enum ParsedWalk<'s> {
    Ordinary((&'s str, Option<i32>)),
    Charm((&'s str, &'s str)),
}

pub(crate) fn parse_walk(input: &str) -> ParserResult<ParsedWalk> {
    alt((
        parse_ordinary_walk.map(|res| ParsedWalk::Ordinary(res)),
        parse_charm_walk.map(|res| ParsedWalk::Charm(res)),
    )).parse(input)
}

pub(crate) fn parse_base_instincts(input: &str) -> ParserResult<i32> {
    let (input, _) = tag("\nBase Instincts take them directly to ").parse(input)?;
    let (input, which) = alt((
        tag("second").map(|_| 2),
        tag("third").map(|_| 3),
        tag("fourth").map(|_| 4), // when fifth base is present
    )).parse(input)?;
    let (input, _) = tag(" base!").parse(input)?;

    Ok((input, which))
}

pub(crate) fn parse_ordinary_walk(input: &str) -> ParserResult<(&str, Option<i32>)> {
    let (input, batter_name) = parse_terminated(" draws a walk.").parse(input)?;

    let (input, base_instincts) = opt(parse_base_instincts).parse(input)?;

    Ok((input, (batter_name, base_instincts)))
}

pub(crate) fn parse_charm_walk(input: &str) -> ParserResult<(&str, &str)> {
    // This will need to be updated if anyone charms in a run
    let (input, batter_name) = parse_terminated(" charms ").parse(input)?;
    let (input, pitcher_name) = parse_terminated("!\n").parse(input)?;
    let (input, _) = tag(batter_name).parse(input)?;
    let (input, _) = tag(" walks to first base.").parse(input)?;

    Ok((input, (batter_name, pitcher_name)))
}

pub(crate) fn parse_inning_end(input: &str) -> ParserResult<(i32, Vec<&str>)> {
    let (input, _) = tag("Inning ").parse(input)?;
    let (input, inning_num) = parse_whole_number(input)?;
    let (input, _) = tag(" is now an Outing.").parse(input)?;
    let (input, lost_triple_threat) = many0(preceded(tag("\n"), parse_terminated(" is no longer a Triple Threat."))).parse(input)?;

    Ok((input, (inning_num, lost_triple_threat)))
}

pub(crate) fn parse_stopped_inhabiting(input: &str) -> ParserResult<&str> {
    parse_terminated(" stopped Inhabiting.").parse(input)
}

pub(crate) fn parse_game_end(input: &str) -> ParserResult<((&str, f32), (&str, f32))> {
    // This is a bit tricky because it's a string of arbitrary words (a team name) followed by an
    // arbitrary number (score)
    let (input, winning_team_name) = take_till(AsChar::is_dec_digit).parse(input)?;
    let (input, winning_team_score) = float(input)?;
    let (input, _) = tag(", ").parse(input)?;
    let (input, losing_team_name) = take_till(AsChar::is_dec_digit).parse(input)?;
    let (input, losing_team_score) = float(input)?;

    pub(crate) fn fix_team(name: &str, score: f32) -> (&str, f32) {
        if let Some(n) = name.strip_suffix(" -") {
            (n, -score)
        } else {
            (name.strip_suffix(" ").unwrap(), score)
        }
    }

    let (winning_team_name, winning_team_score) = fix_team(winning_team_name, winning_team_score.into());
    let (losing_team_name, losing_team_score) = fix_team(losing_team_name, losing_team_score.into());

    // Just checking that my assumption is correct. It's <= because of 20.3
    assert!(losing_team_score <= winning_team_score);

    // The parsers for *_team_name should always leave us with a space at the end
    Ok((input, ((winning_team_name, winning_team_score),
                (losing_team_name, losing_team_score))))
}

pub(crate) enum MildPitchType<'a> {
    Ball((i32, i32)),
    Walk(&'a str),
}

pub(crate) fn parse_mild_pitch_ball(input: &str) -> ParserResult<MildPitchType> {
    // Fun fact: Can't reuse the ball parser because it looks for a comma but this has a period
    let (input, _) = tag("Ball, ").parse(input)?;
    let (input, count) = parse_count(input)?;
    let (input, _) = tag(".").parse(input)?;

    Ok((input, MildPitchType::Ball(count)))
}

pub(crate) fn parse_mild_pitch(input: &str) -> ParserResult<(&str, MildPitchType)> {
    let (input, pitcher_name) = parse_terminated(" throws a Mild pitch!\n").parse(input)?;

    // Fun fact: Can't reuse the ball parser because it looks for a comma but this has a period
    let (input, pitch_type) = alt((
        parse_mild_pitch_ball,
        parse_terminated(" draws a walk.").map(|name| MildPitchType::Walk(name))
    )).parse(input)?;

    Ok((input, (pitcher_name, pitch_type)))
}

pub(crate) fn parse_runners_advance_on_mild_pitch(input: &str) -> ParserResult<bool> {
    let (input, runners_advance) = opt(tag("\nRunners advance on the pathetic play!")).parse(input)?;
    Ok((input, runners_advance.is_some()))
}

pub(crate) fn parse_coffee_bean(input: &str) -> ParserResult<(&str, &str, &str, bool, bool)> {
    let (input, player_name) = parse_terminated(" is Beaned by a ").parse(input)?;
    let (input, roast) = parse_terminated(" roast with ").parse(input)?;
    let (input, notes) = parse_terminated(".\n").parse(input)?;
    let (input, player_name2) = parse_terminated(" is ").parse(input)?;
    assert_eq!(player_name, player_name2);
    let (input, (wired, gained)) = alt((
        tag("Wired!").map(|_| (true, true)),
        tag("no longer Wired!").map(|_| (true, false)),
        tag("Tired.").map(|_| (false, true)),
        tag("no longer Tired!").map(|_| (false, false)),
    )).parse(input)?;

    Ok((input, (player_name2, roast, notes, wired, gained)))
}

pub(crate) fn parse_gain_free_refill(input: &str) -> ParserResult<(&str, &str, &str, &str)> {
    let (input, player_name) = parse_terminated(" is Poured Over with a ").parse(input)?;
    let (input, roast) = parse_terminated(" roast blending ").parse(input)?;
    let (input, ingredient1) = parse_terminated(" and ").parse(input)?;
    let (input, ingredient2) = parse_terminated("!\n").parse(input)?;
    let (input, _) = tag(player_name).parse(input)?;
    let (input, _) = tag(" got a Free Refill.").parse(input)?;

    Ok((input, (player_name, roast, ingredient1, ingredient2)))
}

pub(crate) enum IncinerationBlockedReason {
    Magmatic,
    Fireproof,
}

pub(crate) fn parse_incineration_blocked(input: &str) -> ParserResult<(&str, IncinerationBlockedReason)> {
    let (input, _) = tag("Rogue Umpire tried to incinerate ").parse(input)?;
    let (input, player_name) = parse_terminated(", but ").parse(input)?;
    let (input, blocked_reason) = alt((
        pair(tag(player_name), tag(" ate the flame! They became Magmatic!")).map(|_| IncinerationBlockedReason::Magmatic),
        tag("they're Fireproof! The Umpire was incinerated instead!").map(|_| IncinerationBlockedReason::Fireproof),
    )).parse(input)?;
    Ok((input, (player_name, blocked_reason)))
}

pub(crate) fn parse_player_mod_expires(input: &str) -> ParserResult<(&str, ModDuration)> {
    // This message treats possessives of names ending in s correctly
    let (input, player_name) = parse_terminated_by_possessive.parse(input)?;
    let (input, duration) = alt((
        tag("game").map(|_| ModDuration::Game),
        tag("weekly").map(|_| ModDuration::Weekly),
        tag("seasonal").map(|_| ModDuration::Seasonal),
    )).parse(input)?;
    let (input, _) = tag(" mods wore off.").parse(input)?;
    Ok((input, (player_name, duration)))
}

fn parse_terminated_by_possessive(input: &str) -> ParserResult<&str> {
    alt((
        parse_terminated("'s "),
        parse_terminated("' ")
    )).parse(input)
}

pub(crate) fn parse_team_mod_expires(input: &str) -> ParserResult<(&str, ModDuration)> {
    let (input, _) = tag("The ").parse(input)?;
    // This message treats possessives of names ending in s correctly
    let (input, player_name) = alt((
        parse_terminated("'s "),
        parse_terminated("' ")
    )).parse(input)?;
    let (input, duration) = alt((
        tag("game").map(|_| ModDuration::Game),
        tag("weekly").map(|_| ModDuration::Weekly),
        tag("seasonal").map(|_| ModDuration::Seasonal),
    )).parse(input)?;
    let (input, _) = tag(" mods wore off.").parse(input)?;
    Ok((input, (player_name, duration)))
}

pub(crate) enum ParsedBlooddrainAction<'s> {
    AddBall,
    RemoveBall,
    AddStrike(Option<&'s str>),
    // if there's a strikeout looking, there's a name here
    RemoveStrike,
    AddOut,
    RemoveOut,
}

pub(crate) fn parse_blooddrain_action(drinker_name: &str) -> impl Fn(&str) -> ParserResult<ParsedBlooddrainAction> + '_ {
    move |input: &str| {
        let (input, _) = tag(drinker_name).parse(input)?;
        let (input, action) = alt((
            // preceded(tag(" increased their "), terminated(parse_category, tag(" ability!"))).map(|ability| BlooddrainAction::IncreaseAbility(ability)),
            tag(" adds a Ball!").map(|_| ParsedBlooddrainAction::AddBall),
            tag(" removes a Ball!").map(|_| ParsedBlooddrainAction::RemoveBall),
            preceded(tag(" adds a Strike!\n"), parse_terminated(" strikes out looking.")).map(|name| ParsedBlooddrainAction::AddStrike(Some(name))),
            tag(" adds a Strike!").map(|_| ParsedBlooddrainAction::AddStrike(None)),
            tag(" removes a Strike!").map(|_| ParsedBlooddrainAction::RemoveStrike),
            tag(" adds a Out!").map(|_| ParsedBlooddrainAction::AddOut),
            tag(" removes a Out!").map(|_| ParsedBlooddrainAction::RemoveOut),
        )).parse(input)?;

        Ok((input, action))
    }
}

pub(crate) fn parse_blooddrain_ability<'a>(drinker_name: &'a str, category: &'a str) -> impl Fn(&str) -> ParserResult<()> + 'a {
    move |input: &str| {
        let (input, _) = tag(drinker_name).parse(input)?;
        let (input, _) = tag(" increased their ").parse(input)?;
        let (input, _) = tag(category).parse(input)?;
        let (input, _) = tag(" ability!").parse(input)?;

        Ok((input, ()))
    }
}

pub(crate) fn parse_blooddrain_siphon(input: &str) -> ParserResult<(&str, &str, AttrCategory, Option<ParsedBlooddrainAction>)> {
    let (input, _) = tag("The Blooddrain gurgled!\n").parse(input)?;
    let (input, drinker_name) = parse_terminated("'s Siphon activates!\n").parse(input)?;
    let (input, _) = tag(drinker_name).parse(input)?;
    let (input, _) = tag(" siphoned some of ").parse(input)?;
    let (input, drunk_name) = parse_terminated("'s ").parse(input)?;
    let (input, category) = parse_category(input)?;
    let (input, _) = tag(" ability!\n").parse(input)?;
    let (input, action) = alt((
        parse_blooddrain_action(drinker_name).map(|a| Some(a)),
        parse_blooddrain_ability(drinker_name, &category.to_string()).map(|()| None),
    )).parse(input)?;

    Ok((input, (drinker_name, drunk_name, category, action)))
}

pub(crate) fn parse_category(input: &str) -> ParserResult<AttrCategory> {
    alt((
        tag("hitting").map(|_| AttrCategory::Batting),
        tag("baserunning").map(|_| AttrCategory::Baserunning),
        tag("pitching").map(|_| AttrCategory::Pitching),
        tag("defensive").map(|_| AttrCategory::Defense),
    )).parse(input)
}

pub(crate) fn parse_friend_of_crows(input: &str) -> ParserResult<(Option<&str>, &str)> {
    let (input, pitcher_name) = opt(parse_terminated(" calls upon their Friends!\n")).parse(input)?;
    let (input, _) = tag("A murder of Crows ambush ").parse(input)?;
    let (input, batter_name) = parse_terminated("!\nThey run to safety, resulting in an out.").parse(input)?;

    Ok((input, (pitcher_name, batter_name)))
}

pub(crate) fn parse_black_hole_swallowed_win(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The Black Hole swallowed a Win from the ").parse(input)?;
    let (input, team_name) = parse_terminated("!").parse(input)?;

    Ok((input, team_name))
}

pub(crate) fn parse_sun2_set_win(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Sun 2 set a Win upon the ").parse(input)?;
    let (input, team_name) = parse_terminated(".").parse(input)?;

    Ok((input, team_name))
}

pub(crate) fn parse_sun2(input: &str) -> ParserResult<(&str, Option<&str>)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, scoring_team) = parse_terminated(" collect 10! Sun 2 smiles.\nSun 2 set a Win upon the ").parse(input)?;
    let (input, _) = tag(scoring_team).parse(input)?;
    let (input, _) = tag(".").parse(input)?;
    let (input, rays_player) = opt(preceded(tag("\n"), parse_terminated(" catches some rays."))).parse(input)?;

    Ok((input, (scoring_team, rays_player)))
}

pub(crate) fn parse_black_hole(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, scoring_team) = parse_terminated(" collect 10!\nThe Black Hole swallows the Runs and a ").parse(input)?;
    let (input, victim_team) = parse_terminated(" Win.").parse(input)?;

    Ok((input, (scoring_team, victim_team)))
}

pub(crate) fn parse_team_did_shame(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, shaming_team) = parse_terminated(" shamed the ").parse(input)?;
    let (input, shamed_team) = parse_terminated(".").parse(input)?;

    Ok((input, (shaming_team, shamed_team)))
}

pub(crate) fn parse_team_was_shamed(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, shamed_team) = parse_terminated(" were shamed by the ").parse(input)?;
    let (input, shaming_team) = parse_terminated(".").parse(input)?;

    Ok((input, (shaming_team, shamed_team)))
}

pub(crate) fn parse_allergic_reaction(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" swallowed a stray peanut and had an allergic reaction!").parse(input)?;

    Ok((input, player_name))
}

pub(crate) fn parse_feedback(input: &str) -> ParserResult<(&str, &str, ActivePositionType)> {
    let (input, _) = tag("Reality flickers. Things look different ...\n").parse(input)?;
    let (input, player1_name) = parse_terminated(" and ").parse(input)?;
    let (input, player2_name) = parse_terminated(" switch teams in the feedback!\n").parse(input)?;
    let (input, _) = tag(player2_name).parse(input)?;
    let (input, _) = tag(" is now ").parse(input)?;
    let (input, position) = alt((
        tag("batting").map(|_| ActivePositionType::Lineup),
        tag("pitching").map(|_| ActivePositionType::Rotation),
    )).parse(input)?;
    let (input, _) = tag(".").parse(input)?;

    Ok((input, (player1_name, player2_name, position)))
}

pub(crate) fn parse_perk_up(input: &str) -> ParserResult<Vec<&str>> {
    let (input, names) = separated_list1(tag("\n"), parse_terminated(" Perks up.")).parse(input)?;

    Ok((input, names))
}

pub(crate) fn parse_superyummy(input: &str) -> ParserResult<(&str, bool)> {
    let (input, result) = alt((
        parse_terminated(" loves Peanuts.").map(|n| (n, true)),
        parse_terminated(" misses Peanuts.").map(|n| (n, false)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) fn parse_bestow_reverberating(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Reverberations are at dangerous levels!\n").parse(input)?;
    let (input, player_name) = parse_terminated(" is now Reverberating wildly!").parse(input)?;

    Ok((input, player_name))
}

pub(crate) enum ParsedReverbType {
    Rotation,
    Lineup,
    Full,
    SeveralPlayers,
}

pub(crate) fn parse_roster_shuffle(input: &str) -> ParserResult<(&str, ParsedReverbType, Vec<&str>)> {
    alt((parse_roster_shuffle_unsafe, parse_roster_shuffle_dangerous)).parse(input)
}

pub(crate) fn parse_roster_shuffle_unsafe(input: &str) -> ParserResult<(&str, ParsedReverbType, Vec<&str>)> {
    let (input, _) = tag("Reverberations are at unsafe levels!\nThe ").parse(input)?;
    let (input, (team_name, reverb_type)) = alt((
        parse_terminated(" had their rotation shuffled in the Reverb!").map(|n| (n, ParsedReverbType::Rotation)),
        parse_terminated(" had their lineup shuffled in the Reverb!").map(|n| (n, ParsedReverbType::Lineup)),
        parse_terminated(" had several players shuffled in the Reverb!").map(|n| (n, ParsedReverbType::SeveralPlayers)),
    )).parse(input)?;

    let (input, gravity_players) = many0(preceded(tag("\n"), parse_terminated("'s Gravity kept them in place!"))).parse(input)?;

    Ok((input, (team_name, reverb_type, gravity_players)))
}

pub(crate) fn parse_roster_shuffle_dangerous(input: &str) -> ParserResult<(&str, ParsedReverbType, Vec<&str>)> {
    let (input, _) = tag("Reverberations are at dangerous levels!\nThe ").parse(input)?;
    let (input, team_name) = parse_terminated(" were shuffled in the Reverb!").parse(input)?;

    let (input, gravity_players) = many0(preceded(tag("\n"), parse_terminated("'s Gravity kept them in place!"))).parse(input)?;

    Ok((input, (team_name, ParsedReverbType::Full, gravity_players)))
}

pub(crate) fn parse_become_triple_threat(input: &str) -> ParserResult<Vec<&str>> {
    let (input, names) = alt((
        parse_double_become_triple_threat,
        parse_single_become_triple_threat,
    )).parse(input)?;

    Ok((input, names))
}

pub(crate) fn parse_double_become_triple_threat(input: &str) -> ParserResult<Vec<&str>> {
    let (input, pitcher1_name) = parse_terminated(" and ").parse(input)?;
    let (input, pitcher2_name) = parse_terminated(" chug a Third Wave of Coffee!\nThey are now Triple Threats!").parse(input)?;

    Ok((input, vec![pitcher1_name, pitcher2_name]))
}

pub(crate) fn parse_single_become_triple_threat(input: &str) -> ParserResult<Vec<&str>> {
    let (input, pitcher1_name) = parse_terminated(" chugs a Third Wave of Coffee!\nThey are now a Triple Threat!").parse(input)?;

    Ok((input, vec![pitcher1_name]))
}

pub(crate) fn parse_under_over_over_under(mod_text: &str) -> impl Fn(&str) -> ParserResult<(&str, bool)> + '_ {
    move |input: &str| {
        // complier told me to do the thing with `x` to make the lifetimes work
        let x = alt((
            parse_terminated(&format!(", {mod_text}, On.")).map(|n| (n, true)),
            parse_terminated(&format!(", {mod_text}, Off.")).map(|n| (n, false)),
        )).parse(input);
        x
    }
}

pub(crate) fn parse_taste_the_infinite(input: &str) -> ParserResult<(&str, &str)> {
    let (input, sheller_name) = parse_terminated(" tastes the infinite!\n").parse(input)?;
    let (input, shellee_name) = parse_terminated(" is Shelled!").parse(input)?;

    Ok((input, (sheller_name, shellee_name)))
}

pub(crate) enum ParsedBatterSkippedReason {
    Shelled,
    Elsewhere,
}

pub(crate) fn parse_batter_skipped(input: &str) -> ParserResult<(&str, ParsedBatterSkippedReason)> {
    let (input, result) = alt((
        parse_terminated(" is Shelled and cannot escape!").map(|n| (n, ParsedBatterSkippedReason::Shelled)),
        parse_terminated(" is Elsewhere..").map(|n| (n, ParsedBatterSkippedReason::Elsewhere)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) fn parse_feedback_blocked(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("Reality begins to flicker ...\nBut ").parse(input)?;
    let (input, player1_name) = parse_terminated(" resists!\n").parse(input)?;
    let (input, player2_name) = parse_terminated(" is tangled in the flicker!").parse(input)?;

    Ok((input, (player1_name, player2_name)))
}

pub(crate) fn parse_flag_planted(input: &str) -> ParserResult<(&str, &str, &str, bool)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" break ground on ").parse(input)?;
    let (input, park_name) = parse_terminated(", selecting to build the ").parse(input)?;
    let (input, prefab_name) = parse_terminated(" prefab").parse(input)?;

    let (input, is_first) = alt((
        tag("!\nTHE FLAG IS PLANTED").map(|_| true),
        tag(".\nAnother flag is planted!").map(|_| false),
    )).parse(input)?;

    Ok((input, (team_nickname, park_name, prefab_name, is_first)))
}

pub(crate) fn parse_team_division_move(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" have joined the ILB!\nThey will play in the ").parse(input)?;
    let (input, division_name) = parse_terminated(" division.").parse(input)?;

    Ok((input, (team_nickname, division_name)))
}

pub(crate) enum ParsedPlayerDivisionMove<'a> {
    JoinedIlb(&'a str),
    PulledThroughRift(&'a str),
}

pub(crate) fn parse_player_division_move(input: &str) -> ParserResult<ParsedPlayerDivisionMove> {
    let (input, result) = alt((
        parse_terminated(" has joined the ILB.").map(|n| ParsedPlayerDivisionMove::JoinedIlb(n)),
        parse_terminated(" was pulled through the Rift.").map(|n| ParsedPlayerDivisionMove::PulledThroughRift(n)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) enum ParsedFloodingEffect<'a> {
    Elsewhere(&'a str),
    Flippers(&'a str),
    Ego(&'a str),
}

pub(crate) fn parse_flooding_swept(input: &str) -> ParserResult<(Vec<ParsedFloodingEffect>, bool)> {
    let (input, _) = tag("A surge of Immateria rushes up from Under!\nBaserunners are swept from play!").parse(input)?;
    let (input, effects) = many0(parse_flooding_swept_effect).parse(input)?;

    let (input, flumps) = opt(tag("\nThe Flood Pumps activate!")).parse(input)?;

    Ok((input, (effects, flumps.is_some())))
}

pub(crate) fn parse_flooding_swept_effect(input: &str) -> ParserResult<ParsedFloodingEffect> {
    alt((
        preceded(tag("\n"), parse_terminated(" is swept Elsewhere!"))
            .map(|n| ParsedFloodingEffect::Elsewhere(n)),
        preceded(tag("\n"), parse_terminated(" uses their Flippers to slingshot home!"))
            .map(|n| ParsedFloodingEffect::Flippers(n)),
        preceded(tag("\n"), parse_terminated("'s Ego keeps them on base!"))
            .map(|n| ParsedFloodingEffect::Ego(n)),
    )).parse(input)
}

pub(crate) enum ParsedReturnFromElsewhere<'a> {
    Short(&'a str),
    Normal((&'a str, TimeElsewhere)),
}

pub(crate) fn parse_return_from_elsewhere(input: &str) -> ParserResult<ParsedReturnFromElsewhere> {
    alt((
        parse_terminated(" has returned from Elsewhere!").map(|n| ParsedReturnFromElsewhere::Short(n)),
        parse_normal_return_from_elsewhere.map(|v| ParsedReturnFromElsewhere::Normal(v)),
    )).parse(input)
}

pub(crate) fn parse_normal_return_from_elsewhere(input: &str) -> ParserResult<(&str, TimeElsewhere)> {
    let (input, player_name) = parse_terminated(" has returned from Elsewhere after ").parse(input)?;
    let (input, after_days) = alt((
        tag("one season!").map(|_| TimeElsewhere::Seasons(1)),
        terminated(parse_whole_number, tag(" seasons!")).map(|n| TimeElsewhere::Seasons(n)),
        tag("1 day!").map(|_| TimeElsewhere::Days(1)),
        terminated(parse_whole_number, tag(" days!")).map(|n| TimeElsewhere::Days(n)),
    )).parse(input)?;

    Ok((input, (player_name, after_days)))
}

pub(crate) fn parse_incineration(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("Rogue Umpire incinerated ").parse(input)?;
    let (input, victim_name) = parse_terminated("!\nThey're replaced by ").parse(input)?;
    let (input, replacement_name) = parse_until_period_eof(input)?;

    Ok((input, (victim_name, replacement_name)))
}

pub(crate) fn parse_pitcher_change(input: &str) -> ParserResult<(&str, &str)> {
    let (input, victim_name) = parse_terminated(" is now pitching for the ").parse(input)?;
    let (input, team_name) = parse_until_period_eof(input)?;

    Ok((input, (victim_name, team_name)))
}

pub(crate) fn parse_party(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" is Partying!").parse(input)?;

    Ok((input, player_name))
}

pub(crate) fn parse_player_hatched(input: &str) -> ParserResult<&str> {
    let (input, player_name) = parse_terminated(" has been hatched from the field of eggs.").parse(input)?;

    Ok((input, player_name))
}

pub(crate) enum ParsedPlayerAddedToTeam<'a> {
    PostseasonBirth(&'a str),
    Localized {
        player_name: &'a str,
        team_nickname: &'a str,
        #[allow(unused)] location: &'a str,
    },
}

pub(crate) fn parse_player_added_to_team(input: &str) -> ParserResult<ParsedPlayerAddedToTeam> {
    let (input, team_nickname) = alt((
        preceded(tag("The "), parse_terminated(" earn a Postseason Birth!")).map(|s| ParsedPlayerAddedToTeam::PostseasonBirth(s)),
        parse_player_localized_to_team,
    )).parse(input)?;

    Ok((input, team_nickname))
}

pub(crate) fn parse_player_localized_to_team(input: &str) -> ParserResult<ParsedPlayerAddedToTeam> {
    let (input, player_name) = parse_terminated(" Localized into the ").parse(input)?;
    // Handle proper posessive of team names ending in s
    let (input, team_nickname) = alt((parse_terminated("'s "), parse_terminated("' "))).parse(input)?;
    let (input, location) = alt((tag("lineup"), tag("rotation"))).parse(input)?;
    let (input, _) = tag(".").parse(input)?;

    Ok((input, ParsedPlayerAddedToTeam::Localized {
        player_name,
        team_nickname,
        location,
    }))
}

pub(crate) fn parse_final_standings(input: &str) -> ParserResult<(&str, i32, &str)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" finished ").parse(input)?;
    let (input, place) = parse_whole_number(input)?;
    let (input, _) = match place {
        1 => tag("st").parse(input)?,
        2 => tag("nd").parse(input)?,
        3 => tag("rd").parse(input)?,
        _ => tag("th").parse(input)?,
    };
    let (input, _) = tag(" in the ").parse(input)?;
    let (input, division_name) = parse_until_period_eof(input)?;

    Ok((input, (team_nickname, place - 1, division_name)))
}

pub(crate) enum ParsedRemovedMod<'s> {
    TeamRemovedFromPartyTimeForPostseason(&'s str),
    TeamUsedFreeWill(&'s str),
    PlayerLostMod((&'s str, &'s str)),
    InvestigationConcluded(&'s str),
}

pub(crate) fn parse_removed_mod(input: &str) -> ParserResult<ParsedRemovedMod> {
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" have been removed from Party Time to join the Postseason!"))
            .map(|n| ParsedRemovedMod::TeamRemovedFromPartyTimeForPostseason(n)),
        preceded(tag("The "), parse_terminated(" used their Free Will."))
            .map(|n| ParsedRemovedMod::TeamUsedFreeWill(n)),
        pair(parse_terminated(" lost the "), parse_terminated(" mod."))
            .map(|nm| ParsedRemovedMod::PlayerLostMod(nm)),
        preceded(tag("The Crime Scene Investigation at "), parse_terminated(" has concluded."))
            .map(|r| ParsedRemovedMod::InvestigationConcluded(r)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) enum ParsedAddedMod<'a> {
    EnteredPartyTime(&'a str),
    GainFreeWill(&'a str),
    MVP(&'a str),
}

pub(crate) fn parse_added_mod(input: &str) -> ParserResult<ParsedAddedMod> {
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" have entered Party Time!")).map(|n| ParsedAddedMod::EnteredPartyTime(n)),
        preceded(tag("The "), parse_terminated(" gain Free Will.")).map(|n| ParsedAddedMod::GainFreeWill(n)),
        parse_terminated(" is named an MVP.").map(|n| ParsedAddedMod::MVP(n)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) fn parse_postseason_advance(input: &str) -> ParserResult<(&str, Option<i32>, i32)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" advanced to ").parse(input)?;

    let (input, round_num) = alt((
        preceded(tag("Round "), parse_whole_number).map(|n| Some(n)),
        tag("The Internet Series").map(|_| None),
    )).parse(input)?;
    let (input, _) = tag(" of the Season ").parse(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Postseason.").parse(input)?;

    Ok((input, (team_nickname, round_num, season_num)))
}

pub(crate) fn parse_earned_postseason_slot(input: &str) -> ParserResult<(&str, i32)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" earned a spot in the Season ").parse(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Postseason.").parse(input)?;

    Ok((input, (team_nickname, season_num)))
}

pub(crate) fn parse_postseason_eliminated(input: &str) -> ParserResult<(&str, i32)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" have been eliminated from the Season ").parse(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Postseason.").parse(input)?;

    Ok((input, (team_nickname, season_num)))
}

pub(crate) enum ParsedPlayerStatIncrease<'a> {
    PlayerBoosted(&'a str),
    BottomDwellers(&'a str),
}

pub(crate) fn parse_player_stat_increase(input: &str) -> ParserResult<ParsedPlayerStatIncrease> {
    alt((
        parse_terminated(" was boosted.").map(|name| ParsedPlayerStatIncrease::PlayerBoosted(name)),
        parse_bottom_dweller.map(|name| ParsedPlayerStatIncrease::BottomDwellers(name)),
    )).parse(input)
}

pub(crate) fn parse_bottom_dweller(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_name) = parse_terminated(" are Bottom Dwellers.").parse(input)?;

    Ok((input, team_name))
}

pub(crate) fn parse_team_won_internet_series(input: &str) -> ParserResult<(&str, i32)> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" won the Season ").parse(input)?;
    let (input, season_num) = parse_whole_number(input)?;
    let (input, _) = tag(" Internet Series!").parse(input)?;

    Ok((input, (team_nickname, season_num)))
}

pub(crate) fn parse_will_received(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Will Received: ").parse(input)?;
    // This should take the rest because there shouldn't be any newlines
    let (input, blessing_title) = take_till1(|c| c == '\n').parse(input)?;

    Ok((input, blessing_title))
}

pub(crate) fn parse_blessing_won(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Blessing Won: ").parse(input)?;
    // This should take the rest because there shouldn't be any newlines
    let (input, blessing_title) = take_till1(|c| c == '\n').parse(input)?;

    Ok((input, blessing_title))
}

pub(crate) enum EarlbirdsChange<'a> {
    Added(&'a str),
    Removed, // This one says [object Object]. lol & lmao
}

pub(crate) fn parse_earlbird(input: &str) -> ParserResult<EarlbirdsChange> {
    let (input, _) = tag("Happy Earlseason!\n").parse(input)?;
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" are Earlbirds!")).map(|n| EarlbirdsChange::Added(n)),
        tag("Earlbirds wears off for the [object Object].").map(|_| EarlbirdsChange::Removed),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) enum LateToThePartyChange<'a> {
    Added(&'a str),
    Removed(&'a str), // This one does not say [object Object]
}

pub(crate) fn parse_late_to_the_party(input: &str) -> ParserResult<LateToThePartyChange> {
    let (input, _) = tag("Late to the Party!\n").parse(input)?;
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" are Late to the Party!")).map(|n| LateToThePartyChange::Added(n)),
        preceded(tag("Late to the Party wears off for the "), parse_terminated(".")).map(|n| LateToThePartyChange::Removed(n)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) fn parse_decree_passed(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("Decree Passed: ").parse(input)?;
    // This should take the rest because there shouldn't be any newlines
    let (input, decree_title) = take_till1(|c| c == '\n').parse(input)?;

    Ok((input, decree_title))
}

pub(crate) fn parse_blooddrain(input: &str) -> ParserResult<(&str, &str, AttrCategory)> {
    let (input, _) = tag("The Blooddrain gurgled!\n").parse(input)?;
    let (input, drinker_name) = parse_terminated(" siphoned some of ").parse(input)?;
    let (input, drunk_name) = parse_terminated("'s ").parse(input)?;
    let (input, category) = parse_category(input)?;
    let (input, _) = tag(" ability!\n").parse(input)?;
    let (input, _) = parse_blooddrain_ability(drinker_name, &category.to_string()).parse(input)?;

    Ok((input, (drinker_name, drunk_name, category)))
}

pub(crate) fn parse_undersea(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_name) = parse_terminated(" go Undersea. They're now Overperforming!").parse(input)?;

    Ok((input, team_name))
}

pub(crate) fn parse_peanut_mister(input: &str) -> ParserResult<(&str, bool)> {
    let (input, _) = tag("The Peanut Mister activates!\n").parse(input)?;
    let (input, result) = alt((
        parse_terminated(" has been cured of their peanut allergy!").map(|n| (n, false)),
        parse_terminated(" is no longer Superallergic!").map(|n| (n, true)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) fn parse_birds_unshell(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The Birds circle...\nThe Birds pecked ").parse(input)?;
    let (input, player_name) = parse_terminated(" free!").parse(input)?;

    Ok((input, player_name))
}

pub(crate) fn parse_player_replaces_returned(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("The ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" cut a player and promoted another from the shadows.").parse(input)?;

    Ok((input, team_nickname))
}

pub(crate) fn parse_high_pressure(input: &str) -> ParserResult<(&str, bool)> {
    let (input, _) = tag("The pressure is ").parse(input)?;
    let (input, is_on) = alt((tag("on!").map(|_| true), tag("off.").map(|_| false))).parse(input)?;
    let (input, _) = tag(" The ").parse(input)?;
    let (input, team_nickname) = if is_on {
        parse_terminated(" are Overperforming.").parse(input)?
    } else {
        parse_terminated(" are no longer Overperforming.").parse(input)?
    };

    Ok((input, (team_nickname, is_on)))
}


pub(crate) fn parse_echo(input: &str) -> ParserResult<(&str, &str)> {
    let (input, echoer_name) = parse_terminated(" Echoed ").parse(input)?;
    let (input, echoee_name) = parse_terminated("!").parse(input)?;

    Ok((input, (echoer_name, echoee_name)))
}


pub(crate) fn parse_echo_into_static(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("ECHO ").parse(input)?;
    let (input, echoer_name) = parse_terminated(" STATIC\nECHO ").parse(input)?;
    let (input, echoee_name) = parse_terminated(" STATIC").parse(input)?;

    Ok((input, (echoer_name, echoee_name)))
}


pub(crate) fn parse_psychoacoustics(input: &str) -> ParserResult<(&str, &str, &str)> {
    let (input, stadium_name) = parse_terminated(" is Resonating.\nPsychoAcoustics Echo ").parse(input)?;
    let (input, mod_name) = parse_terminated(" at the ").parse(input)?;
    let (input, team_nickname) = parse_terminated(".").parse(input)?;

    Ok((input, (stadium_name, mod_name, team_nickname)))
}


pub(crate) fn parse_echo_receiver(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("ECHO ").parse(input)?;
    let (input, echoer_name) = parse_terminated(" ECHO ").parse(input)?;
    let (input, echoee_name) = parse_terminated(" ECHO").parse(input)?;

    Ok((input, (echoer_name, echoee_name)))
}


pub(crate) fn parse_consumer_attack(input: &str) -> ParserResult<(&str, Option<&str>, bool)> {
    let (input, _) = tag("CONSUMERS ATTACK\n").parse(input)?;
    let (input, scattered) = opt(tag("SCATTERED\n")).parse(input)?;
    let (input, victim_name) = take_till1(|c| c == '\n').parse(input)?;
    let (input, item_breaks) = opt(preceded(tag("\n\n"), parse_terminated(" BREAKS"))).parse(input)?;

    Ok((input, (victim_name, item_breaks, scattered.is_some())))
}

pub(crate) fn parse_repeat_mvp(input: &str) -> ParserResult<(&str, i32)> {
    let (input, player_name) = parse_terminated(" is named a ").parse(input)?;
    let (input, n_times) = parse_whole_number(input)?;
    let (input, _) = match n_times {
        // Why...
        3 => { tag("-Time MVP!").parse(input)? }
        _ => { tag("-Time MVP.").parse(input)? }
    };

    Ok((input, (player_name, n_times)))
}


pub(crate) fn parse_homebody(input: &str) -> ParserResult<Vec<(&str, bool)>> {
    separated_list1(tag("\n"), parse_single_homebody).parse(input)
}

pub(crate) fn parse_single_homebody(input: &str) -> ParserResult<(&str, bool)> {
    let (input, result) = alt((
        parse_terminated(" is homesick.").map(|n| (n, false)),
        parse_terminated(" is happy to be home.").map(|n| (n, true)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) struct ParsedTeamRunsLost<'a> {
    pub(crate) runs: i32,
    pub(crate) name: &'a str,
}

pub(crate) enum ParsedSalmonRunsLost<'a> {
    None,
    OneTeam(ParsedTeamRunsLost<'a>),
    BothTeams((ParsedTeamRunsLost<'a>, ParsedTeamRunsLost<'a>)),
}

pub(crate) fn parse_salmon(input: &str) -> ParserResult<(i32, ParsedSalmonRunsLost)> {
    let (input, _) = tag("The Salmon swim upstream!\nInning ").parse(input)?;
    let (input, inning_num) = parse_whole_number(input)?;
    let (input, _) = tag(" begins again.").parse(input)?;

    let (input, runs_lost) = alt((
        pair(parse_team_runs_lost, parse_team_runs_lost).map(|rs| ParsedSalmonRunsLost::BothTeams(rs)),
        parse_team_runs_lost.map(|r| ParsedSalmonRunsLost::OneTeam(r)),
        tag("\nNo Runs are lost.").map(|_| ParsedSalmonRunsLost::None),
    )).parse(input)?;

    Ok((input, (inning_num, runs_lost)))
}

pub(crate) fn parse_team_runs_lost(input: &str) -> ParserResult<ParsedTeamRunsLost> {
    let (input, _) = tag("\n").parse(input)?;
    let (input, runs) = parse_whole_number(input)?;
    let (input, _) = tag(" of the ").parse(input)?;
    let (input, name) = parse_terminated("'s Runs are lost!").parse(input)?;

    Ok((input, ParsedTeamRunsLost { runs, name }))
}

pub(crate) fn parse_hit_by_pitch(input: &str) -> ParserResult<(&str, &str)> {
    let (input, pitcher_name) = parse_terminated(" hits ").parse(input)?;
    let (input, batter_name) = parse_terminated(" with a pitch!\n").parse(input)?;
    let (input, _) = tag(batter_name).parse(input)?;
    let (input, _) = tag(" is now being Observed...").parse(input)?; // I'll deal with murder debt later

    Ok((input, (pitcher_name, batter_name)))
}

pub(crate) fn parse_solar_panels(input: &str) -> ParserResult<(f32, &str)> {
    let (input, _) = tag("The Solar Panels absorb Sun 2's energy!\n").parse(input)?;
    let (input, num_runs) = float.parse(input)?;
    let (input, _) = tag(" Runs are collected and saved for the ").parse(input)?;
    let (input, team_nickname) = parse_terminated("'s next game.").parse(input)?;

    Ok((input, (num_runs, team_nickname)))
}

pub(crate) fn parse_runs_overflowing(input: &str) -> ParserResult<(&str, f32, bool)> {
    let (input, _) = tag("Runs are Overflowing!\n").parse(input)?;
    let (input, team_nickname) = parse_terminated(" gain ").parse(input)?;
    let (input, num_runs) = float.parse(input)?;
    let (input, unruns) = alt((
        tag(" Run").map(|_| false),
        tag(" Unrun").map(|_| true),
    )).parse(input)?;
    let (input, _) = opt(tag("s")).parse(input)?;
    let (input, _) = tag(".").parse(input)?;

    Ok((input, (team_nickname, num_runs, unruns)))
}

pub(crate) fn parse_middling(input: &str) -> ParserResult<(&str, bool)> {
    let (input, _) = tag("Happy Midseason!\n").parse(input)?;
    let (input, result) = alt((
        preceded(tag("The "), parse_terminated(" are Middling!")).map(|m| (m, true)),
        preceded(tag("Middling wears off for the "), parse_terminated(".")).map(|m| (m, false)),
    )).parse(input)?;

    Ok((input, result))
}

pub(crate) fn parse_enter_crime_scene(input: &str) -> ParserResult<(&str, &str)> {
    let (input, player_name) = parse_terminated(" enters the Crime Scene at ").parse(input)?;
    let (input, team_nickname) = parse_terminated(" to Investigate...").parse(input)?;

    Ok((input, (player_name, team_nickname)))
}

pub(crate) enum ParsedPlayerMoved<'a> {
    ReturnFromInvestigation((&'a str, bool)),
    Roamin(&'a str),
}

pub(crate) fn parse_player_moved(input: &str) -> ParserResult<ParsedPlayerMoved> {
    alt((
        parse_return_from_investigation.map(|r| ParsedPlayerMoved::ReturnFromInvestigation(r)),
        parse_terminated(" wandered to a new team.").map(|n| ParsedPlayerMoved::Roamin(n)),
    )).parse(input)
}

pub(crate) fn parse_return_from_investigation(input: &str) -> ParserResult<(&str, bool)> {
    let (input, player_name) = parse_terminated(" returns from the Investigation").parse(input)?;
    let (input, emptyhanded) = alt((
        tag(" emptyhanded.").map(|_| true),
        tag(".").map(|_| false),
    )).parse(input)?;

    Ok((input, (player_name, emptyhanded)))
}

pub(crate) enum ParsedGrindRailSuccess<'a> {
    Safe(ParsedGrindRailTrick<'a>),
    TaggedOut(ParsedGrindRailTrick<'a>),
    Bailed,
}

pub(crate) fn parse_grind_rail(input: &str) -> ParserResult<(&str, ParsedGrindRailTrick, ParsedGrindRailSuccess)> {
    let (input, player_name) = parse_terminated(" hops on the Grind Rail toward third base.\nThey do a ").parse(input)?;
    let (input, first_trick) = parse_grind_rail_trick.parse(input)?;
    let (input, _) = tag("!\n").parse(input)?;
    let (input, success) = alt((
        preceded(tag("They land a "), terminated(parse_grind_rail_trick, tag("!\nSafe!")))
            .map(|t| ParsedGrindRailSuccess::Safe(t)),
        preceded(tag("They're tagged out doing a "), terminated(parse_grind_rail_trick, tag("!")))
            .map(|t| ParsedGrindRailSuccess::TaggedOut(t)),
        tag("... but lose their balance and bail!\nOut!").map(|_| ParsedGrindRailSuccess::Bailed),
    )).parse(input)?;


    Ok((input, (player_name, first_trick, success)))
}

pub(crate) struct ParsedGrindRailTrick<'a> {
    pub(crate) name: &'a str,
    pub(crate) score: i32,
}

pub(crate) fn parse_grind_rail_trick(input: &str) -> ParserResult<ParsedGrindRailTrick> {
    // Currently assumes a trick name can't have a "(". I would like to remove this limitation but
    // I couldn't easily figure it out with Nom
    let (input, name) = parse_terminated(" (").parse(input)?;
    let (input, score) = parse_whole_number.parse(input)?;
    let (input, _) = tag(")").parse(input)?;

    Ok((input, ParsedGrindRailTrick { name, score }))
}

pub(crate) fn parse_echo_chamber(input: &str) -> ParserResult<(&str, EchoChamberModAdded)> {
    let (input, _) = tag("The Echo Chamber traps a wave.\n").parse(input)?;
    let (input, player_name) = parse_terminated(" is temporarily ").parse(input)?;
    let (input, mod_) = alt((
        tag("Repeating!").map(|_| EchoChamberModAdded::Repeating),
        tag("Reverberating!").map(|_| EchoChamberModAdded::Reverberating),
    )).parse(input)?;

    Ok((input, (player_name, mod_)))
}

pub(crate) fn parse_item_damage_unknown_name<'a>(extra_space: bool, newline: bool) -> impl FnMut(&'a str) -> ParserResult<(&'a str, &'a str)> {
    move |input| {
        let (input, _) = if newline { tag("\n").parse(input)? } else { (input, "") };
        let (input, _) = if extra_space { tag(" ").parse(input)? } else { (input, "") };
        let (input, player_name) = alt((parse_terminated("'s "), parse_terminated("' "))).parse(input)?;
        let (input, item_name) = parse_terminated(" broke!").parse(input)?;

        Ok((input, (item_name, player_name)))
    }
}

pub(crate) fn parse_item_damage<'a>(player_name: &str, extra_space: bool) -> impl FnMut(&'a str) -> ParserResult<&'a str> + '_ {
    move |input| {
        let (input, _) = if extra_space { tag("\n ") } else { tag("\n") }.parse(input)?;
        let (input, _) = tag(player_name).parse(input)?;
        let (input, _) = alt((tag("'s "), tag("' "))).parse(input)?;
        let (input, item_name) = parse_terminated(" broke!").parse(input)?;

        Ok((input, item_name))
    }
}

pub(crate) fn parse_glitter(input: &str) -> ParserResult<(&str, &str, Option<(&str, bool)>)> {
    let (input, _) = tag("A shimmering Crate descends.\n").parse(input)?;
    let (input, player_name) = parse_terminated(" gained ").parse(input)?;
    // Ditched is when the item is broken, dropped is when it isn't.
    let (input, gained_with_loss) = opt(alt((
        parse_terminated(" and dropped ").map(|s| (s, false)),
        parse_terminated(" and ditched ").map(|s| (s, true)),
    ))).parse(input)?;
    let (input, (gained, lost)) = if let Some((gained, was_broken)) = gained_with_loss {
        let (input, lost) = parse_terminated(".").parse(input)?;
        (input, (gained, Some((lost, was_broken))))
    } else {
        let (input, gained) = parse_terminated(".").parse(input)?;
        (input, (gained, None))
    };

    Ok((input, (player_name, gained, lost)))
}

pub(crate) fn parse_item_restored(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("\n").parse(input)?;
    let (input, player_name) = parse_terminated_by_possessive.parse(input)?;
    let (input, item_name) = parse_terminated(" was restored!").parse(input)?;

    Ok((input, (player_name, item_name)))
}

pub(crate) fn parse_carcinization(input: &str) -> ParserResult<(&str, &str)> {
    let (input, _) = tag("\nThe ").parse(input)?;
    let (input, team_name) = parse_terminated(" steal ").parse(input)?;
    let (input, player_name) = parse_terminated(" for the remainder of the game.").parse(input)?;

    Ok((input, (team_name, player_name)))
}

pub(crate) fn parse_compressed_by_gamma(input: &str) -> ParserResult<&str> {
    let (input, _) = tag("\nThe Black Hole burps!\n").parse(input)?;
    let (input, player_name) = parse_terminated(" is compressed by gamma!").parse(input)?;

    Ok((input, player_name))
}