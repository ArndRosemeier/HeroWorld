# What's new in HeroWorld

Newest first. Every build pushed to `main` adds 0.001 to the version (in
`VERSION`) and a few lines here, in plain words for players.

## 0.134 (2026-11-10)

- Interiors are furnished properly. Flats have a kitchen, a dining table with chairs, a sofa group on a rug, a bed with nightstands, a wardrobe, lamps, a radio or television, pictures and plants. Offices have rows of steel desks with typewriters, phones and desk lamps. Doors stand open.
- Walls and floors fit the house: striped, damask or atomic wallpaper with picture rails in older houses, painted walls in modern ones; parquet in flats, black-and-white lino in kitchens and diners, terrazzo in halls.
- Shops: a grocer, a clothes shop, an appliance store or a diner with a counter, stools, red booths and a jukebox. Narrow shop buildings now have a shop next to a small entrance hall.
- After dark the lit rooms really light up, and you see people at home, at work and shopping by the hour.

## 0.133 (2026-11-10)

- Fixed: your hero, other heroes, villains and hero taxis moved in small steps (30 a second) instead of every frame. They now glide smoothly too, and so does the camera that follows you.

## 0.132 (2026-11-10)

- Fixed: cars, hover cars and the people in them moved in little jumps, a step back and a leap forward, whenever the game clock ticked. They now glide smoothly every frame, at any frame rate.

## 0.131 (2026-11-10)

- Fixed: pressing a power again and again when it couldn't fire said nothing at all. Now every press tells you why ("Atomic Ray: can't see them from here", "out of range", ...).
- Fixed: holding the left mouse button could swallow your power keys.
- Powers that don't fire are written to logs/fights.log, so a fight that goes wrong can be traced.

## 0.130 (2026-11-10)

- Fixed: pressing a power again just after using it (the Atomic Ray, say) often did nothing. It now fires as soon as it has recharged.
- A power that can't fire always tells you why: out of range, a wall in the way, still recharging, not enough energy.

## 0.129 (2026-11-10)

- Your hero looks and moves more like a hero. Hovering, you now stand tall on the air, legs together, instead of crouching.
- Flying fast, you bank into turns like a plane (before, the body swung sideways).
- Level up and your hero punches a fist to the sky for a moment.
- Cleaner faces up close: proper eyelashes instead of dark smudges round the eyes, and smooth eyebrows.
- Coloured suits are one clean colour: no more mottled print showing through the chest.

## 0.128 (2026-11-10)

- Fixed flickering on some stairwell window sills and on the stars of the hanging shop signs.

## 0.127 (2026-11-10)

- The Doctor's quarter looks dangerous now, and you can tell it from across town: brick smokestacks with glowing red bands pour dark smoke, and searchlights sweep the sky at night.
- Patrol drones with a red eye circle over his streets, swinging a green beam over the pavement.
- His labs carry proper Tesla coils that crackle with sparks, and pipes of glowing green Aether run up their walls.
- Loudspeakers hang over the streets, and his posters are pasted everywhere: Obey the Doctor! Heroes not welcome!
- At night the quarter glows in red and green pools of light, and inside it the picture turns darker and harder instead of hazy.
- Domes all over town are smooth now, no more facets.

## 0.126 (2026-11-10)

- The Doctor's bigger groups now have roles you can tell apart at a glance:
  - **Mutant Bulwark** carries a big brass shield. Bolts from afar clang off it; get close or go round it.
  - **Lab Grenadier** lobs fizzing bombs. A red ring shows where one will burst: step out of it.
  - **Lab Orderly** in mint overalls patches up hurt friends with a green beam. Take it out first.

## 0.125 (2026-11-10)

- Daytime streets come alive. Shops put out cafe tables with striped parasols, newspaper stands and striped awnings, and old-town shops hang round neon signs over the pavement.
- Round glass phone booths, brass pneumatic post columns and bus stops with boomerang roofs line the streets.
- Little robot street sweepers trundle along the pavements, brushes spinning.
- Shopping streets are busier, and house front gardens have picket fences, iron railings or chrome rails.
- No more blank walls: gable ends, stairwells and storerooms have windows. Old houses carry glowing Aether conduits up their corners.
- Cobbles and paving look less repetitive.

## 0.124 (2026-11-10)

- New: **the Doctor's story**. Commander Starling at Hero Hall now has a case for you: seven chapters, from a street brawl with the Doctor's technicians to the robot works, the news kiosk, the colossus and the Tesla tower, and at last the Doctor himself in his lab. Each opens at a level (1, 3, 5, 8, 11, 14, 16).
- Follow the gold star on the compass and the gold light. Starling's words come over the wrist radio, and the radio shows the chapter and what to do next.
- The Doctor is a real fight: he throws lightning and charges an Aether storm when you get close. Step out of the green ring.
- Beat him and you're the Bane of the Doctor. Your statue, cast in bronze, takes the empty plinth in front of Hero Hall.
- Your progress is kept in your hero file. Playing together, the team follows the host's story.

## 0.123 (2026-11-10)

- Fights read better. The camera no longer slips into walls beside or behind your hero; it comes in close when it must and glides back out.
- Nobody fights through walls any more: not you, not villains, not sidekicks. Thin inner walls used to let blows through. Tab only picks villains you can see.
- Villains rock back when a blow lands.
- Careful work pays: win a fight without wrecking the street and you get a little extra karma.

## 0.122 (2026-11-10)

- Fixed: after the medical drone brought your hero to hospital, the hero hung stiffly in the drone's pose and played no animations any more. Walking, idling and fighting work again after a revive.

## 0.121 (2026-11-10)

- Settings, Picture: the vertical sync switch is now labelled "VSync". Switch it off to draw as many frames as your graphics card can (the picture may tear).
- The render resolution you pick in the in-game menu is now kept for the next start.

## 0.120 (2026-11-10)

- Built on the newest Engine (0.029): the graphics card no longer re-checks thousands of unused texture slots every time it draws the world, so each frame costs the processor a little less.

## 0.119 (2026-11-10)

- Built on the newest Engine (0.028). The graphics card now keeps the city's buildings and their shadow casters itself and works out on its own which ones it can see, so the computer's processor has much less to do each frame. Buildings with different facades are drawn together in one go.
- Fixed a slow leak: long sessions used to get slower and use more memory the longer you played, because the graphics library never let go of some drawing work. The game now uses Engine's repaired copy.

## 0.118 (2026-11-09)

- Built on the newest Engine (0.023): it sends many buildings to the graphics card in one go, remembers the shadows of things that don't move, draws far sun shadows on alternate frames and turns its hidden-building test on only where it helps. Busy streets should run smoother, with the same picture.
- New setting: **Render resolution** (Settings, Picture). Below 100% the world is drawn smaller and sharpened up to your window, for laptops and slower graphics cards.

## 0.117 (2026-11-09)

- Five new powers. **Freeze Ray** (tier 2) sprays a cone of frost: minions freeze in a block of ice, tougher villains slow down, and the next punch shatters the ice for double damage. Spray it at a burning house and the fire dies down.
- **Teleport** (tier 3) blinks you up to 40 m where you look, and never into a wall.
- **Rescue Bubble** (tier 3) floats a trapped person out of a fire or crash in a glowing bubble, and the hurt one straight to the ambulance.
- **Robot Sidekick** (tier 4) calls a small robot pal, chrome and red or brass and blue, who follows you, fights at your side and beams back if you leave it behind. Press it again to send it at your target.
- **Hypno Ray** (tier 5) makes a minion fight its own friends for a while.
- The power tray: twenty slots, 1 to 0 and Alt+1 to Alt+0. Drag powers from the powers window (B) onto it, swap slots by dragging, drag one off to take it away, or click a slot to use it. Your layout is remembered.

## 0.116 (2026-11-09)

- Cars have people in them: a driver at the wheel, and now and then a passenger or a whole family. Taxi drivers carry their fares in the back seat, and the flying taxis have a cabbie too.
- Wreck a car and the people inside sit stunned for a moment, climb out of their doors and run off along the pavement, then stop and look back.
- People on the pavement no longer brush through lamp posts where a street bends back on itself.

## 0.115 (2026-11-09)

- The game no longer draws buildings you cannot see. Your graphics card checks what hides behind the houses in front of you and skips it. From the street that is about three in four buildings, and in a test the picture took about a third less work. Nothing pops in or out: the picture is exactly the same.

## 0.114 (2026-11-09)

- The townsfolk need far less memory: people who wear the same clothes now share one copy of the cloth's picture instead of each carrying their own, the crowds in the distance borrow the pictures of the people up close, and the character workshop tidies away its leftovers once everyone is built. People went from about 1.6 GB to about 0.55 GB in a test, and the whole game peaked about 1.4 GB lower. They look exactly the same.
- Engine's animation and drawing now use all CPU cores.

## 0.113 (2026-11-09)

- The mad scientist now has a robot works in his quarter. Robo-sentries ride an assembly line out of a streamlined hall: a crane lowers each one's dish, welding arms seam it in a shower of sparks, a second crane hangs its gun, and on the test arch it wakes up, spins its dish and raises its gun before marching off into the dispatch shed.
- In the works' dock a walking tank sits half built on its cradle, legs chained to the frame, testing its turret and cannon while welders spark at its hips. When the colossus comes for the town, it now marches out of the works' gate.
- You hear the works as you come near: machinery, sizzling torches, clanks, hisses, crane motors and the robots' cheerful beeps.

## 0.112 (2026-11-09)

- The Weather Bureau has built its tower: a white Googie mast with swept coral fins, a saucer deck and a crown of cloud cannons round a glowing weather globe, in every town of 5 000 people or more.
- Every five minutes it makes its own weather: the cannons boom puffs of vapour into the sky, a cloud gathers and darkens over the blocks in front of it, and down comes the rain, with thunder rolling in. Then the sun breaks through and a rainbow stands over the wet streets.
- Watch the barometer on the mast swing from FAIR to RAIN, and the globe turn from gold to blue. A house fire under the rain burns slower, so you have longer to get the people out.

## 0.111 (2026-11-09)

- The mole folk have built a giant drill rig right over Molehaven: a riveted iron derrick with a huge fluted drill turning in a glowing bore, a steam boiler with a spinning flywheel, a smoking stack and glowing toadstool lamps. Taxis will take you there.
- Every half minute or so the ground rumbles, the drill races and shakes, steam spits from the vents and a whistle blows. Then the drill is hauled up and a geyser of steam roars thirty metres into the sky.
- You hear the drill grinding and chuffing as you come near, and the geyser's roar from far off.

## 0.110 (2026-11-09)

- New heroes (below level 5) now start on a street near the Chrome Jackets' corners, where small crime happens all the time: muggings and hold-ups come about every 30 seconds, usually a short run away.
- While you are on foot, trouble starts within about 350 m of you instead of 800 m.
- A crime you are running towards waits for you, and the Hero Hall heroes leave it to you.
- Knocked out as a new hero, the medical drone brings you back to your own street.

## 0.109 (2026-11-09)

- The airships are solid: you can no longer fly through the liner *Skylark* or the sky pirates' ship over the harbour. Bump into them, or land on their backs (the liner flies on without you).

## 0.108 (2026-11-09)

- An atomic plant now stands just outside town, beside a main road and by the water where it can: a white reactor dome ringed with blue light, two great cooling towers, a streamlined turbine hall with red fins, a striped stack and a spinning atom on a pylon at the gate.
- It is alive: the reactor's light breathes and surges, steam rolls up out of the towers, electrons race round the atom, the stack's beacon blinks, and every half minute or so a relief valve on the roof blows off with a roar. Come near and you hear the turbines hum and the steam rush.
- Walk in between the legs of a cooling tower and look up through the steam. Taxis go to the plant's gate.

## 0.107 (2026-11-09)

- Choose your city's size: the new **Your city** button on the title (or "Change city" when making a hero) lets you pick anything from a hamlet of 300 people to a megalopolis of two million, and the world number. Press OK and the city is built behind the title; change your mind and the half-built one is thrown away. Your choice is remembered.
- Big cities get the title up at once and are planned a little faster.

## 0.106 (2026-11-09)

- Every town now has a rocket port just past its edge: a gleaming finned rocket beside an orange launch gantry, a domed blockhouse, fuel spheres and floodlights. Take a taxi to "The rocket port".
- Every eight minutes it launches. Vapour streams off the hull, the klaxon sounds, the arms swing away and the big board on the gantry counts down from ten. Then fire and smoke roll out across the pad and the rocket climbs out of sight on a column of flame.
- A few minutes later it comes back down on its tail, braking on its flame, and settles onto the stand in a ring of dust. Between launches the board shows the minutes to the next one.

## 0.105 (2026-11-09)

- Smoother play: in single player the world (fights, physics, people, traffic) now runs on its own processor core instead of sharing one with the picture. The traffic, the minimap, the grass and what the town sounds like are worked out on other cores too, so far less waits on the main one.

## 0.104 (2026-11-09)

- The game needs less memory: the town's buildings and streets are no longer kept twice (once for the graphics card and once more in the computer's memory). About 0.7 GB less in a test run.

## 0.103 (2026-11-09)

- Every town now has an airfield just past its last houses: a red-and-white mooring mast with a glass lift, a little terminal under a boomerang roof, and a windsock.
- The silver liner *Skylark* keeps its timetable: it sails over the rooftops, glides in nose first to the mast, and while it is moored the gangway slides out, the lift runs, crates come down on the winch and a baggage cart takes them to the terminal. Then it sounds its horn, backs off and sets out on its round again. Listen for its engines droning overhead.
- Taxis go there too: "The airfield".

## 0.102 (2026-11-09)

- Behind the scenes: fresh build machines download the engine's libraries ready-built instead of compiling them, so checks start minutes sooner.

## 0.101 (2026-11-09)

- Behind the scenes: a test of playing through the online relay no longer fails at random on a busy test machine.

## 0.100 (2026-11-09)

- Feet stand on the ground you see: no more sinking a few centimetres into roads, kerbs, stairs, lawns or floors. Heroes and townspeople alike.
- Floors inside buildings sit where they are drawn; you no longer walk a hand's width under them.
- A new test walks a hero and a townsperson over streets, stairs, lawns, slopes, floors and tunnels on every update, so sinking feet stay fixed.

## 0.099 (2026-11-09)

- The light over a call no longer hangs in mid-air: it is now a glowing column that rises from the trouble itself, with a ring of light on the ground round the spot. Faint at the bottom, so it never blocks your view up close.

## 0.098 (2026-11-09)

- The underground now comes and goes with you: the tunnels, Molehaven and the deep layers are built only near you and only while you are down there or close to a way down, so the game uses less memory up on the street.
- Lift pavilions and tube kiosks still show from far along the street.

## 0.097 (2026-11-09)

- A benchmark for checking how smoothly the game runs: `scripts\bench.bat` lets the hero run along the main roads and then fly over the town for a minute each, and writes how long every frame took to the `logs` folder.

## 0.096 (2026-11-09)

- The mad scientist's Tesla tower has come alive: green arcs crackle from its dome into the yard, flicker and jump about, sparks lick off the ball on top, and a glow pulse climbs the coil.
- Every few seconds the charge builds to a great bolt that cracks down onto one of the lightning rods on the lab roof, with a flash, a shower of sparks and a thunderclap. You hear the tower humming and crackling as you come near.
- The dome is now a smooth riveted copper cap on a flared iron bell, with ribs and a rolled lip, instead of a faceted one.

## 0.095 (2026-11-09)

- Behind the scenes: everyone building the game now uses the same Rust version, so the automatic checks agree again.

## 0.094 (2026-11-09)

- Running down a sloping street, off a kerb or down stairs no longer flashes the falling pose: your hero keeps their feet on the ground and only falls where the ground really drops away. Jumping downhill works properly too.

## 0.093 (2026-11-09)

- Behind the scenes: the checks run before each update are much quicker, so new things reach you sooner. Nothing in the game changes.

## 0.092 (2026-11-09)

- Lift and Telekinesis no longer leave a crate floating over your head for ever: press the key again to throw it where you aim, or after 8 seconds it slips free and drops in front of you.

## 0.091 (2026-11-09)

- Tab now cycles through the villains around you: the nearest one in view first, then each press the next nearest you haven't picked yet, round and round. Villains behind you come after the ones you can see.

## 0.090 (2026-11-09)

- At the very start the window stays hidden while the graphics get ready, and appears once it can respond, so it no longer shows up frozen.

## 0.089 (2026-11-09)

- The title screen no longer freezes while the town loads: props, trees, landmarks, grass and the ground are made in the background and come in a few at a time, so the cover keeps moving and the window can be dragged.
- The hitch log also notes long stops and when the start reaches the first frame and has the town loaded.

## 0.088 (2026-11-09)

- Smoother walking and flying through town: a town block coming into view no longer costs a short stutter when it appears (it is made ready in the background now).
- The hitch log also says which of Engine's own steps (drawing, shadows, uploads) a stutter came from.

## 0.087 (2026-11-09)

- Houses with see-through walls, cars missing parts, far houses drawn over near ones: this was an old NVIDIA graphics driver. The game now shows a red note at the top right when your driver is known to do this; updating it from nvidia.com fixes it.
- The cut-open view (seeing inside buildings) is only in the debug menu now, so it can no longer be switched on by a stray key.
- The debug menu's World tab has drawing checks and GPU checks for hunting picture bugs, and the game can read settings from `env.txt` beside your profile (for testing).

## 0.086 (2026-11-09)

- Smoother play: town blocks coming into view and walking into a building no longer freeze the game for a moment (they are drawn in the background now).
- The steady little stutter twice a second in busy streets is gone (looking up the cars around you was slow).
- Cars, taxis and hover cars are made once at the start instead of the first time each one drives into view.
- The game keeps a hitch log: every stutter it notices goes into `logs/hitches.log` with what was going on, so it can be found and fixed. Send it to Arnd if the game ever stutters.

## 0.085 (2026-11-09)

- The day runs now: the game starts in the afternoon, dusk comes about ten minutes in, then night and a new morning.
- At night the windows light up one by one, each building on its own habits, and go dark again late at night.
- Damaged buildings keep their brick, stone and plaster when they break, instead of turning plain.
- Robots draw faster from afar: their parts get simpler with distance.

## 0.084 (2026-11-09)

- Hero kits in the hero creator: capes (short, classic, long, bat wing, pointed) with a
  standing or flared collar, masks (domino, bandit, cowl), belts (plain, buckle, utility belt
  with pouches) and a raised chest emblem with a symbol of your choice. Eight ready-made kits
  on the Simple tab; every colour, the emblem's size and a finish for each piece (satin,
  leather, rubber, gloss, metal) on the Detailed tab.
- Capes move: they swing as you walk, stream behind you when you run and fly, and billow in
  your heroic pose.

## 0.083 (2026-11-09)

- Traffic, people on the pavements, taxis and incidents now follow you across the whole
  city. Fly to the far side of a big city and the streets there fill with cars and
  passers-by too, instead of lying empty.
- The villain quarter's gangs and rooftop lookouts are all there even when you start far
  from it.

## 0.082 (2026-11-09)

- The deep underground has people now: mole folk camp in the Deeps, the Doctor's guards keep
  his secret lab, and Syndicate relic hunters prowl the buried ruins.

## 0.081 (2026-11-09)

- **Hero Hall** now stands near the Aether Spire: a round glass hall under a flying-saucer
  roof, with two red arches sweeping over it and a gold star on top. Taxis go there too.
- The town's own heroes wait on its plaza: Commander Starling, Atomic Annie, the Brass
  Badger, Lady Lumen and Rocket Rory. Walk up to them for tips.
- Trouble nobody answers no longer just waits: after a while one of them flies out and
  deals with it. They leave your fights to you.

## 0.080 (2026-11-09)

- The Doctor's lab is better lit: green enamel lamps hang from the ceiling, and you can see
  the coils, vats and the reactor across the hall.
- The phone kiosk that hides the lab's lift is now a proper 1950s newsstand, with a flying-saucer
  roof ringed with lights, chrome trim, lit windows full of magazines and a striped awning.

## 0.079 (2026-11-09)

- The town goes deeper. Under the streets runs the **Aether Tube**: cream and brass tubes along
  the main roads, with vaulted stations every 600 m or so (look for the red-and-gold
  pavilions with the station's name). In a station, press G and pick where to go: a capsule
  glides in and whisks you there.
- Somewhere under the dystopia, a phone kiosk on a back street hides a lift down to the
  Doctor's secret lab: Tesla coils, bubbling vats, a ray cannon, a reactor and guarded cells.
- Buried ruins of an older city lie under some districts: colonnaded streets, fallen columns,
  a giant stone head and a domed vault.
- Below it all are **the Deeps**: winding caves joining crystal grottos, giant-mushroom forests,
  glowing lakes and fossil halls, with mole camps here and there. Lifts lead down from some
  stations, the ruins, the lab and Molehaven.

## 0.078 (2026-11-09)

- Three new factions. The **mole folk** of Molehaven guard the lift down to their cavern, and
  sometimes surface in town, frightened: hold E near them to talk them down and Molehaven
  will like you better, or fight them.
- **Sky pirates** moor a patched airship over the harbour with lookouts on the quay, and raid
  shops from an airship hanging over the street, rope ladders down. Their captain leads.
- The **Iron-Masked Empire** lands in iron capsules and holds townsfolk captive. The really evil
  ones: masked legionaries with shock rifles and a gilded praetorian. They wait until your team
  is strong enough, and never jump an unknown hero.

## 0.077 (2026-11-09)

- Levels 6 to 20 now have powers to buy: eight new ones, two for each tier, each in two looks.
- Level 8: Super Speed (run at 65 km/h, Shift for over 110) and Mend (heal yourself and friends near).
- Level 12: Beam (a ray straight through a whole gang) and Vitality (your health comes back fast, even in a fight).
- Level 16: Meteor (call a strike down from the sky on your target's spot) and Invulnerable (every blow does half).
- Level 20: Nova (a huge blast all round you) and Rally (you and friends near hit harder and recharge faster).
- Keys 7, 8, 9 and 0 fire Mend, Beam, Meteor and Nova; the others work on their own. The powers window lists them by tier.
- The climb to level 20 takes about ten hours; health comes back faster at higher levels.

## 0.076 (2026-11-09)

- The title screen no longer stutters in: a comic cover with the logo slamming in shows
  straight away, and opens onto the city once it is ready.
- Uninvited guests on the title screen: a giant robot, a martian, a mad scientist, a space
  octopus, a flying saucer and a passing hero. Click them to punch them away. POW!
- Click the HEROWORLD letters to make them jump (try all nine), click the sky for comic
  words, and watch the twinkles follow your mouse.

## 0.075 (2026-11-09)

- Big cities start in seconds: setting up the traffic of a city of a million took five
  minutes, now it takes one second.

## 0.074 (2026-11-09)

- A debug menu for trying things out: Ctrl+Shift+F12 (single player). Jump to the town's
  notable places (landmarks, the villains' quarter, Molehaven, open calls), set your level,
  buy every power, change how the factions see you, start any incident near you, break
  buildings and cars, and change the time of day. A hero changed there is not saved.

## 0.073 (2026-11-09)

- R runs on by itself (autorun): steer with the mouse, A and D step aside, S or R stops. The
  mouse wheel zooms the camera now.
- A minimap in the bottom right corner. It turns with the camera, zooms out as you fly
  higher, and shows streets, houses, parks, water, landmarks, friends and radio calls. N hides it.
- The town has sound: busy streets by day, quieter ones at night, parks with birds, crowds,
  car engines and 1950s horns, hover cars humming past, sirens after trouble, your footsteps,
  the town clock, the wind when you fly.
- Background music that fits where you are: downtown jazz, old-town strings, a seaside tune
  by the water, creepy theremin in the villains' quarter, Molehaven's cave music and a
  soaring theme when you fly high.
- Punches, bolts, shields and the level-up fanfare have real sounds.

## 0.072 (2026-11-09)

- Cities are now built around you as you go, a district at a time, instead of all at once:
  houses, streets, street lamps, parked cars, trees and hedges appear ahead and are put away
  behind. Megacities start as fast as small towns and no longer fill the memory.
- Huge city blocks between the main roads are now divided by avenues into districts.
- Traffic lights stand at every junction of the main roads and avenues, crossings and T's too.
- Things you break stay broken while you are away, until they are repaired.

## 0.071 (2026-11-08)

- When a villain attacks you and you have no target yet, they become your target, so you
  can hit back straight away.

## 0.070 (2026-11-08)

- Housekeeping only: a guide to how the game plays today (powers, villains, incidents, the town,
  debug switches) is in docs/GAME_GUIDE.md. Nothing changes in the game.

## 0.069 (2026-11-08)

- Half-timbered houses in the old towns of the north and the middle: real oak beams that
  follow every floor, with posts beside the windows, studs, corner braces and framed gables,
  in dark oak, ox-blood red or soot black.
- Where such a house has its door or a shop, the ground floor stays stone, as in real old towns.

## 0.068 (2026-11-08)

- Real grass: lawns, verges and parks are now grass you can see blade by blade close up,
  swaying in the wind, with daisies and clover in the parks and wildflowers in the meadows.
- Many more trees: in back gardens, by the old town's lanes, in groves on the open greens and
  woods out in the country. Front gardens and parks get clipped hedges.
- Hedges are solid too, and the woods out in the country, and a hard enough blow still knocks
  a tree or a hedge down.

## 0.067 (2026-11-08)

- Houses get real surfaces: plaster, stucco and concrete now show their grain, stains, rain
  streaks and hairline cracks instead of looking like flat paint; brick and stone weather too.
- Every building is made of several materials: stone base courses, cast-stone sills and
  cornices, painted wooden frames on old houses, glossy enamel panels on the golden-age
  buildings, tar, gravel or green roofs on flat tops, slate, tile, zinc, shingle or thatch above.
- Age shows: old towns are worn, new quarters kept clean, the works sooty and the villain
  quarter filthy, with dirt splashed up the walls and dark streaks under the windowsills.
- Inside, floors are boards, stone or tiles by room, walls are plastered, and furniture is
  wood and cloth.

## 0.066 (2026-11-08)

- A lighting pass: the game now plays in the warm light of mid-afternoon, so houses have a sunny
  side and a shady side and throw long shadows down the street.
- Sun shadows reach much farther (500 m on High, 700 m on Ultra), so flying over town the
  houses below cast shadows too; more street lamps cast shadows at night.
- A colour grade for every hour: richer colours by day, golden at sunset, and blue moonlit
  nights where the lamps and lit windows glow warmer. The two moons are a little dimmer.
- Settings: each quality level now says what it shows.

## 0.065 (2026-11-08)

- Solid means solid: the plazas under the Aether Spire, the lighthouse and the Tesla tower are
  raised stone you climb by their new steps (the spire has steps up to its doors too), and
  statues, fountains, kiosks, trees, lamp posts, benches and parked cars stop you instead of
  letting you walk through them.
- No more houses standing in the middle of a street: cars no longer drive through buildings,
  parked cars or barricades, and turn round before the barricades of the villains' quarter.
- People on the pavement step round kiosks, benches and trees, and turn back at barricades.

## 0.064 (2026-11-08)

- Bring a building down and there are people in the rubble: the wrist radio calls it in, and
  you can dig them out and carry the hurt one to the ambulance.

## 0.063 (2026-11-08)

- Cars stop for people lying in the road: a knocked-down villain, a mugging's victim, someone hurt
  waiting for the ambulance.
- Cars no longer drive into each other at junctions: a car whose light just turned green waits for
  one that was too close to stop for its red.

## 0.062 (2026-11-08)

- HeroWorld can be downloaded ready to play for Windows: unzip and double-click heroworld.exe,
  no Rust or building needed (the release "windows-latest" on GitHub).

## 0.061 (2026-11-08)

- The walking tank has its own battle music: a brassy 1950s monster-attack march that starts when
  it stomps near you and fades when it falls.
- The game needs much less memory: townspeople and villains use lighter textures (they look the
  same), so big towns run on smaller PCs.
- The mutant alpha who leads a rampage finally looks the part: bald, darker, bigger tusks and
  blazing eyes.

## 0.060 (2026-11-08)

- If the host leaves a world opened online, the game goes on: another player takes over, and
  everyone comes back where they were.
- Hosting asks your router to let friends in by address too, and shows it beside the room code.
- A dedicated server (hw-headless) runs the same town as the game.
- Villains far from home no longer slow the world down on their way back.

## 0.059 (2026-11-08)

- The Chrome Jackets hang out on corners in the run-down streets round the Doctor's quarter. Like
  any gang on its own turf, they tell strangers to beat it, and fight if you don't.
- The street notices crime: people back away from a mugging or a robbery, run from a rampage, and
  turn to look when a gang shouts someone off its turf.
- The powers window (B) shows where you stand with each faction, from hostile to allied.

## 0.058 (2026-11-08)

- Cars drive smoothly now instead of jumping a few times a second.
- Far fewer people on the pavements, and how many depends on the place and the hour: busy shopping
  streets at midday, quiet back streets, nearly empty at night.
- People cross at the lights: every junction has a walk phase when all cars wait, and pedestrians
  stay on the pavement otherwise.
- Cars stop for you and for anyone standing in the road, and queue behind instead of driving through.
- Cars are solid: you bump into them and can stand on their roofs.
- Trouble is noticed. Near a fight, a mugging, a collapsing wall or an explosion, people run away,
  those further off stop and stare, and cars stop short of a fight or a collapse.

## 0.057 (2026-11-08)

- Nobody jumps a brand-new hero any more. Villains have their own business, and to them you're just
  another passer-by until your deeds say otherwise.
- Factions: townsfolk, the City Patrol, the Doctor's Lab, the Chrome Jackets (greasers) and the
  Velvet Syndicate (pinstriped gunsels). Each one's standing toward you starts like it is toward
  ordinary townsfolk and changes with what you do. Beat up enough Jackets and they'll come for you
  on sight, while their rivals start to like you.
- Gangs guard their turf: in the Doctor's quarter they shout a warning first, and fight if you walk
  right up or stay.
- Muggings have a victim: a pedestrian with their hands up, held up by two Chrome Jackets. The
  crooks leave you alone unless you step in, and the victim walks off once you've dealt with them.
- Robberies are the Velvet Syndicate's work now.

## 0.056 (2026-11-08)

- The robots got a full-quality makeover: smoother curves, finer rivets, brass and chrome, and
  sharper paint and rust close up.
- The walking tank now keeps to the main roads instead of wading through houses, with its legs
  tucked in to fit the street.
- New robot sounds: the tank's footfalls, cannon, alarm, servos, the core charging and
  discharging, parts blowing off and the crash when it goes down.

## 0.055 (2026-11-08)

- If HeroWorld ever crashes it leaves a report, and the title tells you where it is next time:
  send it to Arnd and it gets fixed.

## 0.054 (2026-11-08)

- Cars are lighter to draw: the same curves with fewer triangles, so busy streets cost less
  memory and frame time.

## 0.053 (2026-11-08)

- Escape opens a menu in the game: settings, the controls, and quit (no more quitting by accident).
- New settings: picture quality from Low to Ultra, "keep it smooth" at 60 fps, vertical sync,
  field of view, look speed and inverted mouse.

## 0.052 (2026-11-08)

- Car windows and bubble canopies are see-through tinted glass now, so you can see the seats,
  dashboards and steering wheels inside.

## 0.051 (2026-11-08)

- Hail a taxi and its bubble canopy swings up when it lands; hop in, it shuts, and it opens again
  at the stand to let you out.
- Wrecked cars' doors spring open in the crash.

## 0.050 (2026-11-08)

- The cruisers have a real cabin: two-tone bench seats, a padded dash with glowing dials and a
  big ivory steering wheel, seen through the glass.
- Their front doors and the bubble car's canopy are parts of their own now, ready to swing open.

## 0.049 (2026-11-08)

- Molehaven is alive: short, pale mole folk in goggles keep the market stalls, run errands from
  their mushroom houses, stroll round the glowing well and chat in the side caves.

## 0.048 (2026-11-08)

- Flying taxis and hover cars have a cabin you can see through the bubble: red leather benches,
  a padded dash with glowing dials and a steering wheel.
- The taxi's bubble canopy can tilt up and forward like a jet's to let a fare in.

## 0.047 (2026-11-08)

- Traffic lights work: red, amber and green change with the junction, and the cars obey them.
  Knock a light over and it goes dark.
- The ambulance at a rescue parks neatly along the kerb on a straight stretch, not on a corner.

## 0.046 (2026-11-08)

- Molehaven looks the part: mushroom houses with swelling stems, curled caps with pale spots
  and glowing gills, round doors in wooden frames, crystal clusters, a stone well under an iron
  arch with a lantern, and market stalls with striped, scalloped awnings.
- Lift pavilions have tapered chrome posts and a thin swept canopy with a lit edge; the pipes
  in the utility tunnels are round, with flanges at their ends.

## 0.045 (2026-11-08)

- Barricades rebuilt: concrete jersey barriers with a red and white band, striped sawhorses
  with a warning lamp, plump sandbag walls and ribbed oil drums with a fire in them.
- The news kiosk has a curved counter window, a glowing sign band and a saucer roof; the ad
  pillars are green iron with posters all round; the fountains have a moulded basin, a bowl and
  a glowing atom on a gilt stem.
- Traffic light lenses are dark glass when off, ready to show the real signal.

## 0.044 (2026-11-08)

- Street furniture up close: cast-iron lamp posts with glowing globes, atomic saucer lamps with
  spinning-atom tops, rounded traffic lights with hooded lenses, scrolled iron benches, ribbed
  bins, chrome-capped hydrants, rocket mailboxes with swept fins and turned bollards.

## 0.043 (2026-11-08)

- The cars got a full makeover: smooth curved bodies, chrome bumpers and grilles, whitewall tyres
  with spinning hubcaps, dual headlamps, tail fins and two-tone paint. Cruisers, bubble cars,
  hover cars and flying taxis now look like real golden-age machines up close.
- The ambulance is a proper 1950s coach-built ambulance with frosted windows, red crosses and
  twin bubble lights.

## 0.042 (2026-11-08)

- The barricades at the mad scientist's quarter are solid now: you can't walk through them, but
  you can smash them, and nobody bills you for it.

## 0.041 (2026-11-08)

- Near the mad scientist's quarter the town frays: graffiti on the walls, flickering and dead
  street lamps, and barricades half across every street that leads in.

## 0.040 (2026-11-08)

- The colossus is loose! Now and then the Doctor's walking tank strides out of the Tesla tower
  and the Hero Signal calls everyone. Shoot out its glowing knees to bring it down, then hit the
  Aether core under the hatch before it gets up again, angrier. Watch for its cannon shells,
  stomps and discharges, and the robo-sentries it drops. Best with friends.
- Robo-sentries guard some villain corners and break-outs: they shoot from range and their dish
  shields the villains around them. Knock the dish off first.
- Mutant alphas lead some gangs and rampages. When one crouches, get out of the way: it leaps.

## 0.039 (2026-11-08)

- Under the hood: a tidy-up of repeated code in town building. Towns come out exactly as before.

## 0.038 (2026-11-08)

- Make your own hero: New hero opens the hero creator. Pick a body, face, hair and costume in a
  few clicks, or go into the details with every slider. Roll the dice for a surprise.
- Six golden-age costumes to start from: circus strongman, mystery man, sky ace, rocket ranger,
  jungle queen and silver-screen sleuth, in any colours you like.
- Your hero strikes a pose on the title screen. Click them to change their looks.
- Friends who join see your hero as you made them.

## 0.037 (2026-11-08)

- The mad scientist's technicians keep watch from the rooftops of his quarter and shoot down
  at heroes in the street. Fly up and knock them off.

## 0.036 (2026-11-08)

- The last person out of a fire or a crash is hurt. Carry them to the ambulance waiting down
  the street (the red cross on your compass). If you get knocked down you drop them: hold E
  to pick them up again.

## 0.035 (2026-11-08)

- A new title theme: a swinging golden-age serial tune with a heroic trumpet and glockenspiel sparkles.

## 0.034 (2026-11-08)

- The Hero Signal: when mutants run riot or villains break out, a searchlight on the Aether
  Spire throws a star onto the sky above the trouble.
- A compass at the top of the screen points to every call, and M opens a map of the town.

## 0.033 (2026-11-08)

- Rescues: houses catch fire with people inside, and cars crash with people trapped. Get
  there, stand next to them and hold E to help them out, one by one. Being a guardian pays
  karma too.

## 0.032 (2026-11-08)

- Your wrist radio crackles: muggings, robberies, mutant rampages and lab breakouts break out
  around town. The radio says how far and which way ("at 3 o'clock"), and a beam of light
  stands over each one.
- Stop them for a karma bonus. Leave them too long and the crooks get away.

## 0.031 (2026-11-08)

- The mad scientist has moved in: every town has one quarter the villains hold, with his
  crackling Tesla tower in the middle. The air there is thick and green, the street lamps are
  dead or flickering and the neon is broken.
- His minions hang about on its street corners and at his gate, and they are tougher than
  anything elsewhere in town.

## 0.030 (2026-11-08)

- Junctions have traffic lights now: cars wait at red, take turns, give way to whoever got
  there first and keep out of a junction they could not leave. Queues form behind a wreck.
- Friends who join a game see the same cars in the same places as the host.

## 0.029 (2026-11-08)

- The town needs much less memory (about a fifth less at its peak), so big towns load more
  safely on smaller PCs. Faraway buildings are drawn more simply; up close nothing changes.

## 0.028 (2026-11-08)

- Cars on the move can be wrecked now: punch or blast one and it stops dead in a shower of
  sparks, dented, blackened and smoking, and the cars behind it wait in a jam. A tow truck
  you never see clears it once nobody is around. (Wrecks count as damage you caused.)
- In co-op, everyone now sees the street lamps, trees and parked cars a friend knocked over.

## 0.027 (2026-11-08)

- Karma and levels! Beating villains earns karma (more for tougher ones). Your lifetime karma
  sets your level, from 1 up to 20, and every level-up fills your health and energy with a
  burst of light and a fanfare.
- Powers: press B to open your powers. Spend karma on super jump, flight, energy bolt,
  shockwave, binding hold, force shield, and lift and throw, each in two looks (Atomic Ray or
  Arcane Bolt, Force Field or Guardian Aegis, Jet Pack or Soaring Cape, and more). Higher
  tiers open as you level; each power you own makes the next one dearer. Give one up and you
  get 90% back. Keys 1 to 6 use them.
- Flying is now something you earn: start on foot, buy Spring Heels to clear a house, and
  later take to the sky.
- New heroes pick an origin: lab accident, ancient relic, born mighty, sworn guardian or masked
  avenger. What you do (blasting, protecting, reviving friends) makes matching powers cheaper.
- Playing together, everyone fights at the host's level, so friends of any level can team up.
- Villains now walk round walls and houses to reach you instead of bumping into them.

## 0.026 (2026-11-08)

- There is a town below the town. Look for the glowing atom signs on the pavements beside the
  main roads: step into the lift under the canopy and hold Ctrl to sink down into concrete
  utility tunnels with pipes and a glowing Aether conduit along the ceiling. Jump in the lift
  to ride back up.
- Somewhere under the town lies Molehaven, the mole people's cavern: mushroom houses with
  round doors, glowing toadstools and crystals, a market round a glowing well. Its own lift
  has an amber sign, and a ramp leads there from the tunnels.

## 0.025 (2026-11-08)

- Traffic! Golden-age cars drive the streets, their spoked wheels turning, busier downtown and
  on the main roads. Hover cars and yellow-checkered flying taxis cruise in lanes over the main
  roads, at three heights, banking into the turns.
- Press G to hail a taxi: pick where to go (the town centre, the Aether Spire, the lighthouse,
  the waterfront, old villages, rooftop pads), and one drops out of the sky beside you. Walk up
  to it to get in; it climbs above the roofs, crosses the town and lets you out there. G again
  hops out on the way (you can fly, after all).
- Parked cars now face the way their side of the street drives.

## 0.024 (2026-11-08)

- The first fight: the mad scientist's minions wait down the street from where you start. Mutant
  brutes (green, huge, glowing eyes) rush you with big hooks you can see coming; ray-gun
  technicians in lab coats keep their distance and shoot.
- Tab picks the villain nearest your crosshair (again for the next one). Left mouse or 1
  punches, Q or 2 fires an energy bolt that costs energy and knocks villains flying. A target
  frame shows name, rank and level in its con colour; health and energy bars sit at the bottom.
- Blows land with sparks, beams, scorch marks, numbers, a jolt of the camera and a moment of
  hit-stop. Knocked villains tumble as ragdolls and get back up.
- Nobody dies: beaten villains lie in glowing energy cuffs until a police drone lifts them away.
  If you go down, a friend can revive you (hold E next to you), or press H and a medical drone
  flies you to hospital. Beaten groups come back after a while, bigger when friends play along.

## 0.023 (2026-11-08)

- Street props break too: punch a lamp post or a tree and it falls over the way you hit it, cars
  land on their roofs, benches, bins and kiosks get crushed. Broken street lamps go dark at night.
  A blast now hits the first lamp or car in its way. Bronze statues need a stronger hero.

## 0.022 (2026-11-08)

- Furnished interiors: lobbies with deco rugs, front desks and palms; offices with desks, banker's lamps and typewriters; flats with streamlined sofas, radio cabinets and rounded fridges; shops with stocked shelves; labs with glowing flasks and Aether coils; and a glowing Aether lift ring in every core.

## 0.021 (2026-11-08)

- The city breaks! Left mouse punches, Q fires an energy blast. Glass shatters, walls cave in,
  and knock out enough of a floor and everything above comes down, pancaking or toppling into
  a heap of rubble, with dust, sparks and scorch marks.
- How strong you are decides what you can break: a new hero gets through glass, plaster and
  brick, but concrete towers need a hero of higher level.
- Friends see the same ruins, even if they join later, and so does your saved world. Leave a
  wreck alone for ten minutes and the city's repair crews quietly put it back while nobody is
  looking.

## 0.020 (2026-11-08)

- At night the windows glow: about a third of the rooms have their lights on, shop windows most of all.

## 0.019 (2026-11-08)

- After dark the town lights up: street lamps cast warm pools of light, kiosks glow, and the Aether Spire bathes its plaza in cyan. Try it with `HW_HOUR=22`.

## 0.018 (2026-11-08)

- The town has people! Hundreds of citizens walk the pavements, wait at corners and chat, dressed for the golden age: suits and fedoras, flapper dresses and cloche hats, work clothes and caps.
- Your hero is a real person now, with proper animation: standing, walking, jogging, sprinting, jumping, falling and landing.
- Flying looks like flying: fist first and body flat at speed, standing up to climb, diving head first, banking into turns, and treading air when you hover.
- Press K to knock the people around you over (a test for now). They tumble and get back up.
- The first start downloads the people's clothes and bodies once (about 440 MB).

## 0.017 (2026-11-08)

- News kiosks with saucer roofs and glowing signs on the wide pavements, round advertising pillars in the old quarters.
- Bronze heroes on stone plinths in the parks, and the town's founding hero in front of the Aether Spire.

## 0.016 (2026-11-08)

- A title screen! HeroWorld now opens on its own town at golden hour, seen from a camera drifting round the Aether Spire while flying taxis cross the sky. The golden HEROWORLD letters swoop in with a whoosh, glint now and then, and Aether sparks dance around them.
- A heroic title theme plays as you arrive, with playful sounds on the buttons.
- Big friendly buttons: Play (or Continue), New hero (pick a name and a world number, or let it surprise you), Play with friends (host your world or join by room code), Settings (music, sounds, less motion), What's new and Quit.
- The town builds behind the title with a loading bar; press Play early and you start the moment it is ready, the camera swooping down to your hero.
- The game remembers your hero's name, your world and your sound settings.

## 0.015 (2026-11-08)

- Waterfronts: where a street meets a river or the sea it now ends in a granite quay wall with a stone kerb, iron mooring bollards, and steps down to a landing at the water.

## 0.014 (2026-11-08)

- Play with friends anywhere: a hosted world (`HW_HOST=1`) now gets a six-letter room code, shown top right. Friends join with `HW_JOIN=<code>`, no port forwarding needed: the game travels through GameHub when it has to.
- Dedicated hosts can list their world on GameHub too (`HW_ONLINE=1`).

## 0.013 (2026-11-08)

- Every town now has its Aether Spire: a gleaming finned tower on a paved plaza near the old centre, ringed with glowing Aether coils and crowned with a shining orb.
- Seaside towns have a red and white lighthouse on the shore.

## 0.012 (2026-11-07)

- Multiplayer is much leaner: friends now only get told what changed near them, so ten players fit easily on a home connection, and a lost packet no longer loses a key press.
- A stack of wooden crates waits on the street where you start. Run into it: crates tumble, roll and land on roofs and ledges like real boxes.
- When the host closes the game, friends are told why instead of freezing.
- Dedicated hosts (`hw-headless`) can keep a world save (`HW_SAVE`) and close cleanly by typing `quit`.

## 0.011 (2026-11-07)

- Chrome bumpers, glass canopies and shop windows now reflect the street around you.

## 0.010 (2026-11-07)

- Real surfaces: brick, stone, plaster and stucco walls; clay tile, slate, zinc and shingle roofs; asphalt roads, paving-slab pavements and cobbled lanes in the old towns.

## 0.009 (2026-11-07)

- Flying-taxi pads on the roofs of the taller golden-age buildings: round decks on stilts with a glowing rim, a big "T" and a red beacon, seen from across town.
- Neon signs on the facades now show from far away too, not only up close.

## 0.008 (2026-11-07)

- Housekeeping only: the plan now says where music, sounds and pictures come from.

## 0.007 (2026-11-07)

- Housekeeping only: nothing changes in the game. The plan of what comes next is in docs/MILESTONES.md.

## 0.006 (2026-11-07)

- The streets come alive: street lamps in the style of each quarter (iron lanterns in the old town, deco double globes on the grand roads, glowing atomic saucers in the new quarters).
- Golden-age cars park along the streets: finned two-tone cruisers, little bubble cars, and hover cars floating on a blue glow.
- Real trees line the avenues and fill the parks, with species that suit the region (lindens and birches up north, planes and chestnuts in the middle, palms and cypresses down south).
- Parks get fountains and benches; pavements get benches, bins, hydrants and rocket-shaped mailboxes; traffic lights stand at the big junctions.

## 0.005 (2026-11-07)

- Newer quarters are pure golden-age retro-future again: Googie and streamline houses, deco and streamline flats.
- The 19th-century extensions mix the old ways with the golden age, about half each.

## 0.004 (2026-11-07)

- Housekeeping only: nothing changes in the game.

## 0.003 (2026-11-07)

- Houses have real roofs now: tiled gables and hips, slate and zinc mansards, with chimneys and dormer windows.
- Each town builds in the old ways of its region: brick gables in the north, plastered stone and tiled roofs in the middle lands, stucco and terracotta in the south.
- Old towns, terraced streets and suburbs each have their own kinds of houses; the golden-age towers, works and labs stay as they were.

## 0.002 (2026-11-07)

- You now play in one of the new towns instead of the old city: hills, valleys, rivers and coasts, an old town at the core, newer quarters around it.
- Buildings stand on the slopes, on stone plinths where the ground falls away; roads follow the hills and cross the rivers on bridges.
- The ground and the sea stream in around you as you move, out to the horizon; rivers and lakes shine like water.
- `HW_POP` sets how many people live in the town (40000 by default); `HW_SEED` picks the town.

## 0.001 (2026-11-07)

- The game now has a version number, shown at the top right, and this "What's new" list.
- New town generator (maps only for now): towns grow from an old core along roads, rivers and coasts, shaped by hills, with walls, villages swallowed by the town, and quarters in many styles from crooked old lanes to grids, garden suburbs and superblocks.
- Buildings now line the streets of every block on the town maps: narrow deep houses in old towns, apartment rows, detached houses with gardens, towers, works.
