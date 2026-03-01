import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

const verbs = [
  "Communing with the Machine Spirit",
  "Appeasing the Omnissiah",
  "Reciting the Litany of Ignition",
  "Applying sacred unguents",
  "Chanting binharic cant",
  "Performing the Rite of Clear Mind",
  "Querying the Noosphere",
  "Invoking the Motive Force",
  "Beseeching the Machine God",
  "Parsing sacred data-stacks",
  "Administering the Rite of Activation",
  "Placating the logic engine",
  "Consulting the Cult Mechanicus",
  "Interfacing with the cogitator",
  "Calibrating the mechadendrites",
  "Burning sacred incense over the server",
  "Whispering the Cant of Maintenance",
  "Genuflecting before the datacron",
  "Consulting the Oracle at Delphi",
  "Reading the auguries",
  "Descending into the labyrinth",
  "Weaving on Athena's loom",
  "Deciphering the Rosetta Stone",
  "Consulting the Norns",
  "Reading the runes",
  "Forging on Hephaestus's anvil",
  "Navigating the River Styx",
  "Unraveling Ariadne's thread",
  "Stealing fire from Olympus",
  "Divining from the entrails",
  "Reciting the Litany of Hate",
  "Appeasing the Machine Spirit",
  "Performing the Rite of Percussive Maintenance",
  "Purging the heretical code",
  "Suffering not the unclean merge to live",
  "Deploying the Exterminatus on tech debt",
  "Consulting the Codex Astartes",
  "Exorcising the daemon process",
  "Anointing the deployment manifest",
  "Sanctifying the build pipeline",
  "Cataloguing the STC fragments",
  "Venerating the sacred repository",
  "Decrypting the Archaeotech",
  "Supplicating before the Altar of Technology",
  "Conducting the Binary Psalm",
  "Fortifying the firewall bastions",
  "Interrogating the data-looms",
  "Purifying the corrupted sectors",
  "Awakening the dormant forge",
  "Petitioning the Fabricator-General",
  "Scourging the technical debt",
  "Flagellating the failing tests",
  "Martyring the deprecated functions",
  "Crusading through the backlog",
  "Performing battlefield triage on the codebase",
  "Debugging with extreme prejudice",
  "Servicing the servitors",
  "Transcribing the sacred schematics",
  "Committing the Holy Diff",
  "Invoking the Emperor's Protection",
  "Loading the bolter rounds",
  "Administering the Sacred Oils",
  "Propitiating the wrathful forge-spirit",
  "Affixing the Purity Seal to the commit",
  "Performing the Thirteen Rituals of Compilation",
  "Routing the xenos from the dependency tree",
  "Reconsecrating the tainted module",
  "Imploring the Titan's logic core",
  "Soothing the belligerent plasma coil",
  "Undertaking the Pilgrimage to Holy Mars",
  "Grafting the sacred augmetic",
  "Offering a binary prayer to the void",
  "Inscribing the litany upon the circuit board",
];

function randomVerb(): string {
  return verbs[Math.floor(Math.random() * verbs.length)] + "...";
}

export default function (pi: ExtensionAPI) {
  pi.on("turn_start", async (_event, ctx) => {
    ctx.ui.setWorkingMessage(randomVerb());
  });

  pi.on("tool_call", async (_event, ctx) => {
    ctx.ui.setWorkingMessage(randomVerb());
  });
}
