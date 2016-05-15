# Words with friends?
Are you talking to a stranger or your friend?? You tell me!


## How to Play as a stranger

text **stranger join** to _906 563 1337_. We'll add you to a pool of potential strangers.

## How to play as the Guesser

text **who dis** to _906 563 1337_.

We'll wait for a stranger to join the party and then give you a friend code.

## How to play as a friend

Have your guesser friend send you the friend code and text: **join [friend code]** to _906 506 1337_


## How to run
1. setup rust: `curl -sf https://static.rust-lang.org/rustup.sh | sudo sh`
1. setup env vars: `TWILIO_SID` `TWILIO_AUTH` `TWILIO_FROM` (`TWILIO_FROM` is the phone number you'll text to and receive texts from. so 906 506 1337 in this example)
1. run `cargo run`
