# Coins, digging and the shop (v0.9)

Blip is an office desktop pet that sits on the taskbar all day. This adds a small, slow economy:
the pet finds and digs up coins, you spend them on a few things it wears or uses, and buddies
see what yours wears. It must stay **calm**: nice to glance at, never something that pulls you
away from work. Reviewed by a critic pass before building; their changes are folded in below.

## The calm contract

1. **Nothing asks for attention.** No cards, no sounds, no badges, no reminders, no streaks,
   no daily rewards, nothing that expires. Coins never decay; ignoring them loses nothing.
2. **Ignoring it is fine.** A coin on the taskbar gets picked up by the pet itself.
3. **Small on screen.** At most 2 coins lie on the taskbar line (tiny, poop-sized). Dug coins go
   straight into the wallet with a small coin rising over the pet. Digging is a short swing in
   place, no sparks.
4. **Care comes first.** Coins only come while Food, Fun and Clean are 35+, not sick, not asleep.
5. **Only while someone is there.** Earning needs mouse/keyboard input in the last 5 minutes.
   Leaving the PC on overnight earns nothing. Nothing is earned while Blip is closed.
6. **Busy = quiet.** No coins or digging during a Blip call, Do-not-disturb, or when a
   fullscreen app / presentation is in front.
7. **Off switch.** Settings → Pet → "Coins & shop". Off: no coins, no digging, no Shop button.
8. **No surveillance.** Colleagues never see counts (coins, digs, hours). Only what the pet
   wears, its pickaxe, and how many different finds it has. "Show my pet's things to buddies"
   can be switched off.
9. **Finite and honest.** Everything can be had in about six weeks of normal use. No paid
   anything, no loot boxes. New items come with updates.

## Earning

| What | How often (awake, cared for, someone there) | Gives |
| --- | --- | --- |
| Finding | every 25–35 min | 1 coin, dropped on the taskbar next to the pet |
| Digging (needs a pickaxe) | every 20–30 min, a ~3 s swing | wooden 2 · stone 3 · gold 5 · diamond 8, straight to the wallet |
| A find | 3% of digs, any pickaxe | a curio for the Finds shelf (the gem is also worth 25 coins) |

The pet walks over and picks up a coin on the taskbar after a little while; clicking it picks it
up right away. Throwing and petting keep Fun up (which keeps coins coming) but never pay coins.

About 7 cared-for hours a day: wooden ≈ 50/day, stone ≈ 68, gold ≈ 104, diamond ≈ 158.

## The shop

A normal window from the menu's **Shop** button. The header shows the wallet and, when coins
have paused, one muted line saying why ("Blip is bored", "asleep", "you're away"...).

| Item | Kind | Price |
| --- | --- | --- |
| Wooden pickaxe | pickaxe | 15 |
| Stone pickaxe | pickaxe | 150 |
| Gold pickaxe | pickaxe | 450 |
| Diamond pickaxe | pickaxe | 1200 |
| Sunglasses | face | 30 |
| Round glasses | face | 50 |
| Party hat | head | 40 |
| Flower | head | 80 |
| Hard hat | head | 120 |
| Crown | head | 1500 |
| Energy pill | use: +40 Energy now | 20 |
| Sleep pill | use while asleep: this nap recovers 3× faster | 15 |

Pickaxes replace each other (no selling). One hat and one pair of glasses can be worn at a time;
the pet chirps once when it puts on something new. Wearables show in every pose except the
curled-up ball. Wallet, wardrobe and finds belong to the person: they carry over to a new egg.

## Games

Games are add-ons: they are **not** part of Blip. Only someone who gets one in the shop downloads
it (from the GitHub release matching their Blip, checked against its SHA-256); everyone else
never has it on disk, and nothing runs until it's played.

**Hoops** (4 coins). The pet curls into its ball and sits in a slingshot on the ground at the
left of the screen. Pull it back, let go, and get it through a hoop hanging on the right edge of
the monitor (a new height after every basket; always within reach). Ten balls a game; the score
is a small quiet label next to the hoop. Right-click, or the little x by the score, stops.
A basket pays 1 coin (at most 20 coins an hour from games); buddies see your best score.
It runs as its own small program (`blip-hoops`), and Blip hides its pet while you play.

## Finds shelf

Rare curios from digging: bottle cap, old key, snail shell, fossil, gem. Shown on a shelf in the
shop, unfound ones as "?". The number of different finds is the one thing buddies can see.

## Trading

Buddies can swap **wearables**: in the Buddies window, "Swap" on a buddy → pick one of yours and
one of theirs → they get a card "Ana wants to swap her Party hat for your Sunglasses" with
Swap / No thanks. Nothing is ever given away: it's always one for one, both say yes, and an
offer lapses after 5 minutes. Pickaxes, pills and finds can't be traded (they show effort).

## Also in this update

- The menu's three bars get icons (bowl = Food, ball = Fun, moon = Energy).
- The **Play** button goes; playing is throwing and petting. Its place becomes **Shop**. The pet
  never explains itself or gives instructions: people find out by playing.
