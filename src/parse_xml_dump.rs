#![allow(unused)]

use std::path::PathBuf;

use serde::Deserialize;

//use crate::fetch_revisions::ParsedRevision;

/*
Original PHP code:

/**
 * Load file
 *
 * @return string Contents of XML file to convert
 */
public function loadFile()
{
    if (!file_exists($this->filename)) {
        throw new \Exception('Input file does not exist: ' . $this->filename);
    }

    $file = file_get_contents($this->filename);

    return $file
}

/**
 * Load XML contents into variable
 */
public function loadData($xmlData)
{
    if (($xml = new \SimpleXMLElement($xmlData)) === false) {
        throw new \Exception('Invalid XML File.');
    }
    $this->dataToConvert = $xml->xpath('page');

    if ($this->dataToConvert == '') {
        throw new \Exception('XML Data is empty');
    }
}

/**
 * Method to oversee the cleaning, preparation and converting of one page
 *
 * @return void
 */
public function convertData()
{
    // process each page in input file
    foreach ($this->dataToConvert as $node) {
        $fileMeta = $this->retrieveFileInfo($node->xpath('title'));
        $revision = $node->xpath('revision');
        foreach ($revision as $rev) {
            $timestamp = $rev->xpath('timestamp');
            $contributor = $rev->xpath('contributor/username');
            $comment = $rev->xpath('comment');
            if (!($comment))
                $comment = array("..");
        $text = $rev->xpath('text');
        $text = $this->cleanText($text[0], $fileMeta);

        try {
            $text = $this->runPandoc($text);
            $output = $this->getMetaData($fileMeta) . $text;
            if ($fileMeta['filename'] == 'Main_Page')
                $fileMeta['filename'] = 'home';
            $this->message("Converted " . $fileMeta['filename'] . ", revised on " . $timestamp[0] . " by Contributor " . $contributor[0]);
            $this->saveFile($fileMeta, $output);
            $commandText = "cd " . $fileMeta['directory'] . " && git add "
                    . $fileMeta['filename'] . ".md"
                    . " && git commit --author=\""
                    . $this->usermap[
                            (string)$contributor[0]]  . "\" --date="
                    . $timestamp[0] . " -m \""
                    . $comment[0] . "\" "
                    . $fileMeta['filename'] . ".md";
            $this->message($commandText);
            system($commandText);
            $this->counter++;
        } catch (PandocException $e) {
            if (!$this->skiperrors) {
                throw new \Exception($e);
            } else {
                $this->message("Failed converting " . $fileMeta['title'] . ": " . $e->getMessage());
            }
        }
        }
    }
}

*/

#[derive(Debug, Default, Deserialize)]
pub struct MediaWikiDump {
    #[serde(rename = "page")]
    pages: Vec<PageDump>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PageDump {
    title: String,
    #[serde(rename = "revision")]
    revisions: Vec<RevisionDump>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RevisionDump {
    timestamp: String,
    contributor: ContributorDump,
    comment: String,
    text: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ContributorDump {
    username: String,
}

pub fn get_revisions_from_xml(path: PathBuf) -> MediaWikiDump {
    let file = std::fs::File::open(path).unwrap();
    let reader = std::io::BufReader::new(file);

    let result: MediaWikiDump = serde_xml_rs::from_reader(reader).unwrap();

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn test_load_arch_dump() {
        let dump = get_revisions_from_xml(PathBuf::from("test_files/ArchWiki-20230802150007.xml"));
        assert_debug_snapshot!(dump);
    }
}
